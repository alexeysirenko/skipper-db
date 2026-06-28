pub mod ast;
pub mod lexer;

use ast::{
    ColumnDef, CompareOp, CreateTable, DataType, Expr, Insert, Literal, OrderBy, Projection,
    Select, SelectItem, Statement,
};
use lexer::{ParseError, Token, TokenKind, tokenize};

pub fn parse(input: &str) -> Result<Statement, ParseError> {
    let mut parser = Parser {
        tokens: tokenize(input)?,
        pos: 0,
    };
    let statement = parser.parse_statement()?;
    parser.expect_end()?;
    Ok(statement)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        if self.peek_keyword("CREATE") {
            Ok(Statement::CreateTable(self.parse_create_table()?))
        } else if self.peek_keyword("INSERT") {
            Ok(Statement::Insert(self.parse_insert()?))
        } else if self.peek_keyword("SELECT") {
            Ok(Statement::Select(self.parse_select()?))
        } else {
            Err(self.error("expected CREATE, INSERT, or SELECT"))
        }
    }

    fn parse_select(&mut self) -> Result<Select, ParseError> {
        self.expect_keyword("SELECT")?;
        let projection = self.parse_projection()?;
        self.expect_keyword("FROM")?;
        let from = self.parse_name("table name")?;

        let filter = if self.eat_keyword("WHERE") {
            Some(self.parse_expr()?)
        } else {
            None
        };

        let order_by = if self.eat_keyword("ORDER") {
            self.expect_keyword("BY")?;
            let column = self.parse_name("column name")?;
            let descending = if self.eat_keyword("DESC") {
                true
            } else {
                self.eat_keyword("ASC");
                false
            };
            Some(OrderBy { column, descending })
        } else {
            None
        };

        let limit = if self.eat_keyword("LIMIT") {
            Some(self.parse_limit()?)
        } else {
            None
        };

        Ok(Select {
            projection,
            from,
            filter,
            order_by,
            limit,
        })
    }

    fn parse_projection(&mut self) -> Result<Projection, ParseError> {
        if self.eat(&TokenKind::Star) {
            return Ok(Projection::All);
        }
        let mut items = vec![self.parse_select_item()?];
        while self.eat(&TokenKind::Comma) {
            items.push(self.parse_select_item()?);
        }
        Ok(Projection::Items(items))
    }

    fn parse_select_item(&mut self) -> Result<SelectItem, ParseError> {
        let expr = self.parse_expr()?;
        let alias = if self.eat_keyword("AS") {
            Some(self.expect_ident("alias")?)
        } else {
            None
        };
        Ok(SelectItem { expr, alias })
    }

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_and()?;
        while self.eat_keyword("OR") {
            let right = self.parse_and()?;
            left = Expr::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_compare()?;
        while self.eat_keyword("AND") {
            let right = self.parse_compare()?;
            left = Expr::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_compare(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_primary()?;
        match self.eat_compare_op() {
            Some(op) => {
                let right = self.parse_primary()?;
                Ok(Expr::Compare {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                })
            }
            None => Ok(left),
        }
    }

    fn eat_compare_op(&mut self) -> Option<CompareOp> {
        let op = match self.current() {
            TokenKind::Eq => CompareOp::Eq,
            TokenKind::NotEq => CompareOp::NotEq,
            TokenKind::Lt => CompareOp::Lt,
            TokenKind::LtEq => CompareOp::LtEq,
            TokenKind::Gt => CompareOp::Gt,
            TokenKind::GtEq => CompareOp::GtEq,
            _ => return None,
        };
        self.advance();
        Some(op)
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        match self.current() {
            TokenKind::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(&TokenKind::RParen, "')' after expression")?;
                Ok(expr)
            }
            TokenKind::Int(n) => {
                let n = *n;
                self.advance();
                Ok(Expr::Literal(Literal::Int(n)))
            }
            TokenKind::Str(s) => {
                let s = s.clone();
                self.advance();
                Ok(Expr::Literal(Literal::Str(s)))
            }
            TokenKind::Ident(kw) if kw.eq_ignore_ascii_case("NULL") => {
                self.advance();
                Ok(Expr::Literal(Literal::Null))
            }
            TokenKind::Ident(s) if !is_reserved(s) => {
                let s = s.clone();
                self.advance();
                Ok(Expr::Column(s))
            }
            _ => Err(self.error("expected an expression")),
        }
    }

    fn parse_limit(&mut self) -> Result<i64, ParseError> {
        if let TokenKind::Int(n) = self.current() {
            let n = *n;
            self.advance();
            Ok(n)
        } else {
            Err(self.error("expected a number after LIMIT"))
        }
    }

    fn parse_create_table(&mut self) -> Result<CreateTable, ParseError> {
        self.expect_keyword("CREATE")?;
        self.expect_keyword("TABLE")?;
        let table = self.expect_ident("table name")?;
        self.expect(&TokenKind::LParen, "'(' after table name")?;

        let mut columns = vec![self.parse_column_def()?];
        while self.eat(&TokenKind::Comma) {
            columns.push(self.parse_column_def()?);
        }
        self.expect(&TokenKind::RParen, "',' or ')' in column list")?;

        Ok(CreateTable { table, columns })
    }

    fn parse_column_def(&mut self) -> Result<ColumnDef, ParseError> {
        let name = self.expect_ident("column name")?;
        let ty = self.parse_data_type()?;
        let nullable = if self.eat_keyword("NOT") {
            self.expect_keyword("NULL")?;
            false
        } else {
            self.eat_keyword("NULL");
            true
        };
        Ok(ColumnDef { name, ty, nullable })
    }

    fn parse_data_type(&mut self) -> Result<DataType, ParseError> {
        if self.eat_keyword("INT") {
            Ok(DataType::Int)
        } else if self.eat_keyword("TEXT") {
            Ok(DataType::Text)
        } else {
            Err(self.error("expected a column type (INT or TEXT)"))
        }
    }

    fn parse_insert(&mut self) -> Result<Insert, ParseError> {
        self.expect_keyword("INSERT")?;
        self.expect_keyword("INTO")?;
        let table = self.expect_ident("table name")?;

        let columns = if self.eat(&TokenKind::LParen) {
            let mut names = vec![self.expect_ident("column name")?];
            while self.eat(&TokenKind::Comma) {
                names.push(self.expect_ident("column name")?);
            }
            self.expect(&TokenKind::RParen, "')' after column list")?;
            Some(names)
        } else {
            None
        };

        self.expect_keyword("VALUES")?;
        self.expect(&TokenKind::LParen, "'(' before value list")?;
        let mut values = vec![self.parse_literal()?];
        while self.eat(&TokenKind::Comma) {
            values.push(self.parse_literal()?);
        }
        self.expect(&TokenKind::RParen, "')' after value list")?;

        Ok(Insert {
            table,
            columns,
            values,
        })
    }

    fn parse_literal(&mut self) -> Result<Literal, ParseError> {
        match self.current() {
            TokenKind::Int(n) => {
                let n = *n;
                self.advance();
                Ok(Literal::Int(n))
            }
            TokenKind::Str(s) => {
                let s = s.clone();
                self.advance();
                Ok(Literal::Str(s))
            }
            TokenKind::Ident(kw) if kw.eq_ignore_ascii_case("NULL") => {
                self.advance();
                Ok(Literal::Null)
            }
            _ => Err(self.error("expected a value (number, string, or NULL)")),
        }
    }

    fn expect_end(&mut self) -> Result<(), ParseError> {
        self.eat(&TokenKind::Semicolon);
        if matches!(self.current(), TokenKind::Eof) {
            Ok(())
        } else {
            Err(self.error("expected end of statement"))
        }
    }

    fn current(&self) -> &TokenKind {
        &self.tokens[self.pos].kind
    }

    fn advance(&mut self) {
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
    }

    fn peek_keyword(&self, keyword: &str) -> bool {
        matches!(self.current(), TokenKind::Ident(s) if s.eq_ignore_ascii_case(keyword))
    }

    fn eat_keyword(&mut self, keyword: &str) -> bool {
        if self.peek_keyword(keyword) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect_keyword(&mut self, keyword: &str) -> Result<(), ParseError> {
        if self.eat_keyword(keyword) {
            Ok(())
        } else {
            Err(self.error(format!("expected {keyword}")))
        }
    }

    fn expect_ident(&mut self, what: &str) -> Result<String, ParseError> {
        if let TokenKind::Ident(s) = self.current() {
            let s = s.clone();
            self.advance();
            Ok(s)
        } else {
            Err(self.error(format!("expected {what}")))
        }
    }

    fn parse_name(&mut self, what: &str) -> Result<String, ParseError> {
        if let TokenKind::Ident(s) = self.current()
            && !is_reserved(s)
        {
            let s = s.clone();
            self.advance();
            return Ok(s);
        }
        Err(self.error(format!("expected {what}")))
    }

    fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.current() == kind {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: &TokenKind, what: &str) -> Result<(), ParseError> {
        if self.eat(kind) {
            Ok(())
        } else {
            Err(self.error(format!("expected {what}")))
        }
    }

    fn error(&self, message: impl Into<String>) -> ParseError {
        ParseError {
            message: format!("{}, found {}", message.into(), describe(self.current())),
            pos: self.tokens[self.pos].pos,
        }
    }
}

fn is_reserved(word: &str) -> bool {
    const KEYWORDS: [&str; 20] = [
        "SELECT", "FROM", "WHERE", "ORDER", "BY", "LIMIT", "AND", "OR", "NOT", "NULL", "ASC",
        "DESC", "INSERT", "INTO", "VALUES", "CREATE", "TABLE", "INT", "TEXT", "AS",
    ];
    KEYWORDS.iter().any(|k| word.eq_ignore_ascii_case(k))
}

fn describe(kind: &TokenKind) -> String {
    match kind {
        TokenKind::Ident(s) => format!("\"{s}\""),
        TokenKind::Int(n) => n.to_string(),
        TokenKind::Str(s) => format!("'{s}'"),
        TokenKind::Star => "'*'".to_string(),
        TokenKind::Comma => "','".to_string(),
        TokenKind::Semicolon => "';'".to_string(),
        TokenKind::LParen => "'('".to_string(),
        TokenKind::RParen => "')'".to_string(),
        TokenKind::Eq => "'='".to_string(),
        TokenKind::NotEq => "'!='".to_string(),
        TokenKind::Lt => "'<'".to_string(),
        TokenKind::LtEq => "'<='".to_string(),
        TokenKind::Gt => "'>'".to_string(),
        TokenKind::GtEq => "'>='".to_string(),
        TokenKind::Eof => "end of input".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(name: &str, ty: DataType, nullable: bool) -> ColumnDef {
        ColumnDef {
            name: name.to_string(),
            ty,
            nullable,
        }
    }

    #[test]
    fn parses_create_table() {
        let stmt = parse("CREATE TABLE users (id INT, name TEXT, age INT)").unwrap();
        assert_eq!(
            stmt,
            Statement::CreateTable(CreateTable {
                table: "users".to_string(),
                columns: vec![
                    col("id", DataType::Int, true),
                    col("name", DataType::Text, true),
                    col("age", DataType::Int, true),
                ],
            })
        );
    }

    #[test]
    fn parses_create_table_with_nullability() {
        let stmt = parse("CREATE TABLE t (id INT NOT NULL, name TEXT NULL);").unwrap();
        assert_eq!(
            stmt,
            Statement::CreateTable(CreateTable {
                table: "t".to_string(),
                columns: vec![
                    col("id", DataType::Int, false),
                    col("name", DataType::Text, true),
                ],
            })
        );
    }

    #[test]
    fn create_table_is_case_insensitive() {
        assert!(parse("create table t (id int)").is_ok());
    }

    #[test]
    fn parses_insert() {
        let stmt = parse("INSERT INTO users VALUES (1, 'Alice', 20)").unwrap();
        assert_eq!(
            stmt,
            Statement::Insert(Insert {
                table: "users".to_string(),
                columns: None,
                values: vec![
                    Literal::Int(1),
                    Literal::Str("Alice".to_string()),
                    Literal::Int(20),
                ],
            })
        );
    }

    #[test]
    fn parses_insert_with_column_list_and_null() {
        let stmt = parse("INSERT INTO users (id, name) VALUES (1, NULL)").unwrap();
        assert_eq!(
            stmt,
            Statement::Insert(Insert {
                table: "users".to_string(),
                columns: Some(vec!["id".to_string(), "name".to_string()]),
                values: vec![Literal::Int(1), Literal::Null],
            })
        );
    }

    #[test]
    fn rejects_empty_column_list() {
        assert!(parse("CREATE TABLE users ()").is_err());
    }

    #[test]
    fn rejects_missing_column_type() {
        assert!(parse("CREATE TABLE users (id)").is_err());
    }

    #[test]
    fn rejects_insert_without_values() {
        assert!(parse("INSERT INTO users VALUES;").is_err());
    }

    #[test]
    fn rejects_trailing_tokens() {
        assert!(parse("CREATE TABLE t (id INT) garbage").is_err());
    }

    #[test]
    fn rejects_unknown_statement() {
        let e = parse("DROP TABLE users").unwrap_err();
        assert_eq!(e.pos, 0);
    }

    #[test]
    fn does_not_panic_on_empty_input() {
        assert!(parse("").is_err());
    }

    fn select(stmt: Statement) -> Select {
        match stmt {
            Statement::Select(s) => s,
            other => panic!("expected SELECT, got {other:?}"),
        }
    }

    fn column(name: &str) -> Expr {
        Expr::Column(name.to_string())
    }

    fn int(n: i64) -> Expr {
        Expr::Literal(Literal::Int(n))
    }

    fn cmp(left: Expr, op: CompareOp, right: Expr) -> Expr {
        Expr::Compare {
            left: Box::new(left),
            op,
            right: Box::new(right),
        }
    }

    #[test]
    fn parses_select_star() {
        let s = select(parse("SELECT * FROM users").unwrap());
        assert_eq!(s.projection, Projection::All);
        assert_eq!(s.from, "users");
        assert_eq!(s.filter, None);
        assert_eq!(s.order_by, None);
        assert_eq!(s.limit, None);
    }

    fn col_item(name: &str) -> SelectItem {
        SelectItem {
            expr: Expr::Column(name.to_string()),
            alias: None,
        }
    }

    #[test]
    fn parses_select_columns() {
        let s = select(parse("SELECT id, name FROM users").unwrap());
        assert_eq!(
            s.projection,
            Projection::Items(vec![col_item("id"), col_item("name")])
        );
    }

    #[test]
    fn parses_where_comparison() {
        let s = select(parse("SELECT id FROM users WHERE age > 18").unwrap());
        assert_eq!(s.filter, Some(cmp(column("age"), CompareOp::Gt, int(18))));
    }

    #[test]
    fn and_binds_tighter_than_or() {
        let s = select(parse("SELECT * FROM t WHERE a = 1 OR b = 2 AND c = 3").unwrap());
        assert_eq!(
            s.filter,
            Some(Expr::Or(
                Box::new(cmp(column("a"), CompareOp::Eq, int(1))),
                Box::new(Expr::And(
                    Box::new(cmp(column("b"), CompareOp::Eq, int(2))),
                    Box::new(cmp(column("c"), CompareOp::Eq, int(3))),
                )),
            ))
        );
    }

    #[test]
    fn parentheses_override_precedence() {
        let s = select(parse("SELECT * FROM t WHERE (a = 1 OR b = 2) AND c = 3").unwrap());
        assert_eq!(
            s.filter,
            Some(Expr::And(
                Box::new(Expr::Or(
                    Box::new(cmp(column("a"), CompareOp::Eq, int(1))),
                    Box::new(cmp(column("b"), CompareOp::Eq, int(2))),
                )),
                Box::new(cmp(column("c"), CompareOp::Eq, int(3))),
            ))
        );
    }

    #[test]
    fn parses_order_by_and_limit() {
        let s = select(
            parse("SELECT id, name FROM users WHERE age > 18 ORDER BY name DESC LIMIT 10").unwrap(),
        );
        assert_eq!(
            s.order_by,
            Some(OrderBy {
                column: "name".to_string(),
                descending: true,
            })
        );
        assert_eq!(s.limit, Some(10));
    }

    #[test]
    fn parses_string_equality() {
        let s = select(parse("SELECT id FROM users WHERE name = 'Alice'").unwrap());
        assert_eq!(
            s.filter,
            Some(cmp(
                column("name"),
                CompareOp::Eq,
                Expr::Literal(Literal::Str("Alice".to_string())),
            ))
        );
    }

    #[test]
    fn rejects_select_without_projection() {
        assert!(parse("SELECT FROM users").is_err());
    }

    #[test]
    fn rejects_select_without_from() {
        assert!(parse("SELECT id users").is_err());
    }

    #[test]
    fn rejects_where_without_expression() {
        assert!(parse("SELECT id FROM users WHERE").is_err());
    }

    #[test]
    fn rejects_limit_without_number() {
        assert!(parse("SELECT * FROM t LIMIT").is_err());
    }
}
