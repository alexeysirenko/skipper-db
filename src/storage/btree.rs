use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::error::{Error, Result};
use crate::storage::page::PAGE_SIZE;
use crate::storage::table::Rid;
use crate::storage::{FORMAT_VERSION, MAGIC};

// small fanout so splits show up early; on-disk format doesn't depend on it
const MAX_KEYS: usize = 4;

const LEAF_TAG: u8 = 0;
const INTERNAL_TAG: u8 = 1;

// one node per page; page 0 holds metadata
#[derive(Debug, Clone, PartialEq, Eq)]
enum Node {
    Leaf {
        keys: Vec<i64>,
        rids: Vec<Rid>,
        next: u32,
    },
    Internal {
        keys: Vec<i64>,
        children: Vec<u32>,
    },
}

impl Node {
    fn to_page(&self) -> [u8; PAGE_SIZE] {
        let mut buf = [0u8; PAGE_SIZE];
        match self {
            Node::Leaf { keys, rids, next } => {
                buf[0] = LEAF_TAG;
                buf[1..3].copy_from_slice(&(keys.len() as u16).to_le_bytes());
                buf[3..7].copy_from_slice(&next.to_le_bytes());
                let mut at = 7;
                for (key, rid) in keys.iter().zip(rids) {
                    buf[at..at + 8].copy_from_slice(&key.to_le_bytes());
                    buf[at + 8..at + 12].copy_from_slice(&rid.page.to_le_bytes());
                    buf[at + 12..at + 14].copy_from_slice(&rid.slot.to_le_bytes());
                    at += 14;
                }
            }
            Node::Internal { keys, children } => {
                buf[0] = INTERNAL_TAG;
                buf[1..3].copy_from_slice(&(keys.len() as u16).to_le_bytes());
                let mut at = 3;
                for key in keys {
                    buf[at..at + 8].copy_from_slice(&key.to_le_bytes());
                    at += 8;
                }
                for child in children {
                    buf[at..at + 4].copy_from_slice(&child.to_le_bytes());
                    at += 4;
                }
            }
        }
        buf
    }

    fn from_page(buf: &[u8; PAGE_SIZE]) -> Result<Node> {
        let count = u16::from_le_bytes([buf[1], buf[2]]) as usize;
        match buf[0] {
            LEAF_TAG => {
                if 7 + count * 14 > PAGE_SIZE {
                    return Err(Error::MalformedIndex);
                }
                let next = u32::from_le_bytes(buf[3..7].try_into().unwrap());
                let mut keys = Vec::with_capacity(count);
                let mut rids = Vec::with_capacity(count);
                let mut at = 7;
                for _ in 0..count {
                    keys.push(i64::from_le_bytes(buf[at..at + 8].try_into().unwrap()));
                    let page = u32::from_le_bytes(buf[at + 8..at + 12].try_into().unwrap());
                    let slot = u16::from_le_bytes(buf[at + 12..at + 14].try_into().unwrap());
                    rids.push(Rid { page, slot });
                    at += 14;
                }
                Ok(Node::Leaf { keys, rids, next })
            }
            INTERNAL_TAG => {
                if 3 + count * 8 + (count + 1) * 4 > PAGE_SIZE {
                    return Err(Error::MalformedIndex);
                }
                let mut at = 3;
                let mut keys = Vec::with_capacity(count);
                for _ in 0..count {
                    keys.push(i64::from_le_bytes(buf[at..at + 8].try_into().unwrap()));
                    at += 8;
                }
                let mut children = Vec::with_capacity(count + 1);
                for _ in 0..count + 1 {
                    children.push(u32::from_le_bytes(buf[at..at + 4].try_into().unwrap()));
                    at += 4;
                }
                Ok(Node::Internal { keys, children })
            }
            _ => Err(Error::MalformedIndex),
        }
    }
}

pub struct BTree {
    file: File,
    root: u32,
}

impl BTree {
    pub fn create(path: &Path) -> Result<BTree> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)?;
        let mut tree = BTree { file, root: 1 };
        tree.write_meta()?;
        tree.write_node(
            1,
            &Node::Leaf {
                keys: Vec::new(),
                rids: Vec::new(),
                next: 0,
            },
        )?;
        Ok(tree)
    }

    pub fn open(path: &Path) -> Result<BTree> {
        let mut file = OpenOptions::new().read(true).write(true).open(path)?;
        let mut meta = [0u8; PAGE_SIZE];
        file.read_exact(&mut meta)?;
        if meta[0..8] != MAGIC {
            return Err(Error::NotASkipperDb(path.to_path_buf()));
        }
        let version = u32::from_le_bytes(meta[8..12].try_into().unwrap());
        if version != FORMAT_VERSION {
            return Err(Error::UnsupportedVersion(version));
        }
        let root = u32::from_le_bytes(meta[12..16].try_into().unwrap());
        Ok(BTree { file, root })
    }

    pub fn find(&mut self, key: i64) -> Result<Option<Rid>> {
        let mut page = self.root;
        loop {
            match self.read_node(page)? {
                Node::Leaf { keys, rids, .. } => {
                    return Ok(keys.iter().position(|k| *k == key).map(|i| rids[i]));
                }
                Node::Internal { keys, children } => {
                    page = children[child_index(&keys, key)];
                }
            }
        }
    }

    pub fn insert(&mut self, key: i64, rid: Rid) -> Result<()> {
        if let Some((sep, right)) = self.insert_into(self.root, key, rid)? {
            let new_root = Node::Internal {
                keys: vec![sep],
                children: vec![self.root, right],
            };
            let root_page = self.alloc_node(&new_root)?;
            self.set_root(root_page)?;
        }
        Ok(())
    }

    // Some((separator, right page)) when the node split and the parent must absorb it
    fn insert_into(&mut self, page: u32, key: i64, rid: Rid) -> Result<Option<(i64, u32)>> {
        match self.read_node(page)? {
            Node::Leaf {
                mut keys,
                mut rids,
                next,
            } => {
                match keys.binary_search(&key) {
                    Ok(i) => {
                        rids[i] = rid;
                        self.write_node(page, &Node::Leaf { keys, rids, next })?;
                        return Ok(None);
                    }
                    Err(i) => {
                        keys.insert(i, key);
                        rids.insert(i, rid);
                    }
                }

                if keys.len() <= MAX_KEYS {
                    self.write_node(page, &Node::Leaf { keys, rids, next })?;
                    return Ok(None);
                }

                let mid = keys.len() / 2;
                let right = Node::Leaf {
                    keys: keys.split_off(mid),
                    rids: rids.split_off(mid),
                    next,
                };
                let sep = match &right {
                    Node::Leaf { keys, .. } => keys[0],
                    _ => unreachable!(),
                };
                let right_page = self.alloc_node(&right)?;
                self.write_node(
                    page,
                    &Node::Leaf {
                        keys,
                        rids,
                        next: right_page,
                    },
                )?;
                Ok(Some((sep, right_page)))
            }
            Node::Internal {
                mut keys,
                mut children,
            } => {
                let i = child_index(&keys, key);
                let Some((sep, right_page)) = self.insert_into(children[i], key, rid)? else {
                    return Ok(None);
                };
                keys.insert(i, sep);
                children.insert(i + 1, right_page);

                if keys.len() <= MAX_KEYS {
                    self.write_node(page, &Node::Internal { keys, children })?;
                    return Ok(None);
                }

                let mid = keys.len() / 2;
                let sep_up = keys[mid];
                let right = Node::Internal {
                    keys: keys.split_off(mid + 1),
                    children: children.split_off(mid + 1),
                };
                keys.pop();
                let right_page = self.alloc_node(&right)?;
                self.write_node(page, &Node::Internal { keys, children })?;
                Ok(Some((sep_up, right_page)))
            }
        }
    }

    fn read_node(&mut self, page: u32) -> Result<Node> {
        let mut buf = [0u8; PAGE_SIZE];
        self.file
            .seek(SeekFrom::Start(page as u64 * PAGE_SIZE as u64))?;
        self.file.read_exact(&mut buf)?;
        Node::from_page(&buf)
    }

    fn write_node(&mut self, page: u32, node: &Node) -> Result<()> {
        self.file
            .seek(SeekFrom::Start(page as u64 * PAGE_SIZE as u64))?;
        self.file.write_all(&node.to_page())?;
        self.file.sync_all()?;
        Ok(())
    }

    fn alloc_node(&mut self, node: &Node) -> Result<u32> {
        let page = (self.file.metadata()?.len() / PAGE_SIZE as u64) as u32;
        self.write_node(page, node)?;
        Ok(page)
    }

    fn set_root(&mut self, root: u32) -> Result<()> {
        self.root = root;
        self.write_meta()
    }

    fn write_meta(&mut self) -> Result<()> {
        let mut meta = [0u8; PAGE_SIZE];
        meta[0..8].copy_from_slice(&MAGIC);
        meta[8..12].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
        meta[12..16].copy_from_slice(&self.root.to_le_bytes());
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&meta)?;
        self.file.sync_all()?;
        Ok(())
    }
}

// equal keys descend right, where the separator points them
fn child_index(keys: &[i64], key: i64) -> usize {
    keys.partition_point(|k| key >= *k)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn rid(k: i64) -> Rid {
        Rid {
            page: k as u32 + 1,
            slot: k as u16,
        }
    }

    #[test]
    fn leaf_page_roundtrip() {
        let node = Node::Leaf {
            keys: vec![1, 5, 9],
            rids: vec![rid(1), rid(5), rid(9)],
            next: 42,
        };
        assert_eq!(Node::from_page(&node.to_page()).unwrap(), node);
    }

    #[test]
    fn internal_page_roundtrip() {
        let node = Node::Internal {
            keys: vec![10, 20],
            children: vec![1, 2, 3],
        };
        assert_eq!(Node::from_page(&node.to_page()).unwrap(), node);
    }

    #[test]
    fn from_page_rejects_unknown_tag() {
        let mut buf = [0u8; PAGE_SIZE];
        buf[0] = 9;
        assert!(matches!(Node::from_page(&buf), Err(Error::MalformedIndex)));
    }

    #[test]
    fn find_on_empty_tree_is_none() {
        let dir = tempdir().unwrap();
        let mut tree = BTree::create(&dir.path().join("k.idx")).unwrap();
        assert_eq!(tree.find(1).unwrap(), None);
    }

    #[test]
    fn insert_then_find() {
        let dir = tempdir().unwrap();
        let mut tree = BTree::create(&dir.path().join("k.idx")).unwrap();
        tree.insert(7, rid(7)).unwrap();
        assert_eq!(tree.find(7).unwrap(), Some(rid(7)));
        assert_eq!(tree.find(8).unwrap(), None);
    }

    #[test]
    fn reinserting_key_replaces_rid() {
        let dir = tempdir().unwrap();
        let mut tree = BTree::create(&dir.path().join("k.idx")).unwrap();
        tree.insert(7, rid(7)).unwrap();
        tree.insert(7, rid(99)).unwrap();
        assert_eq!(tree.find(7).unwrap(), Some(rid(99)));
    }

    #[test]
    fn many_inserts_trigger_splits_and_stay_findable() {
        let dir = tempdir().unwrap();
        let mut tree = BTree::create(&dir.path().join("k.idx")).unwrap();
        for k in 0..200 {
            tree.insert(k, rid(k)).unwrap();
        }
        for k in 0..200 {
            assert_eq!(tree.find(k).unwrap(), Some(rid(k)));
        }
        assert_eq!(tree.find(200).unwrap(), None);
    }

    #[test]
    fn descending_inserts_stay_findable() {
        let dir = tempdir().unwrap();
        let mut tree = BTree::create(&dir.path().join("k.idx")).unwrap();
        for k in (0..100).rev() {
            tree.insert(k, rid(k)).unwrap();
        }
        for k in 0..100 {
            assert_eq!(tree.find(k).unwrap(), Some(rid(k)));
        }
    }

    #[test]
    fn index_persists_after_reopen() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("k.idx");
        {
            let mut tree = BTree::create(&path).unwrap();
            for k in 0..50 {
                tree.insert(k, rid(k)).unwrap();
            }
        }
        let mut tree = BTree::open(&path).unwrap();
        for k in 0..50 {
            assert_eq!(tree.find(k).unwrap(), Some(rid(k)));
        }
    }
}
