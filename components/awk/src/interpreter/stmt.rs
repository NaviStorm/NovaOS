use alloc::{string::String, vec::Vec};
use no_std_io2::io::Write;

use crate::ast::*;
use crate::error::Result;
use crate::value::Value;

use super::Interpreter;

/// Result of executing a statement
pub enum StmtResult {
    Normal,
    Break,
    Continue,
    Return(Value),
}

impl<'a> Interpreter<'a> {
    pub fn execute_block<W: Write>(&mut self, block: &Block, output: &mut W) -> Result<StmtResult> {
        for stmt in &block.statements {
            let result = self.execute_stmt(stmt, output)?;
            if self.should_exit || self.should_next || self.should_nextfile {
                return Ok(result);
            }
            match result {
                StmtResult::Normal => continue,
                other => return Ok(other),
            }
        }
        Ok(StmtResult::Normal)
    }

    pub fn execute_stmt<W: Write>(&mut self, stmt: &Stmt, output: &mut W) -> Result<StmtResult> {
        match stmt {
            Stmt::Empty => Ok(StmtResult::Normal),

            Stmt::Expr(expr) => {
                self.eval_expr_with_output(expr, output)?;
                Ok(StmtResult::Normal)
            }

            Stmt::Print {
                args,
                output: redirect,
                ..
            } => {
                self.execute_print(args, redirect, output)?;
                Ok(StmtResult::Normal)
            }

            Stmt::Printf {
                format,
                args,
                output: redirect,
                ..
            } => {
                self.execute_printf(format, args, redirect, output)?;
                Ok(StmtResult::Normal)
            }

            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let cond = self.eval_expr_with_output(condition, output)?;
                if cond.is_truthy() {
                    self.execute_stmt(then_branch, output)
                } else if let Some(else_stmt) = else_branch {
                    self.execute_stmt(else_stmt, output)
                } else {
                    Ok(StmtResult::Normal)
                }
            }

            Stmt::While {
                condition, body, ..
            } => {
                while !(self.should_exit || self.should_next || self.should_nextfile) {
                    let cond = self.eval_expr_with_output(condition, output)?;
                    if !cond.is_truthy() {
                        break;
                    }
                    match self.execute_stmt(body, output)? {
                        StmtResult::Normal | StmtResult::Continue => continue,
                        StmtResult::Break => break,
                        StmtResult::Return(v) => return Ok(StmtResult::Return(v)),
                    }
                }
                Ok(StmtResult::Normal)
            }

            Stmt::DoWhile {
                body, condition, ..
            } => {
                while !(self.should_exit || self.should_next || self.should_nextfile) {
                    match self.execute_stmt(body, output)? {
                        StmtResult::Normal | StmtResult::Continue => {}
                        StmtResult::Break => break,
                        StmtResult::Return(v) => return Ok(StmtResult::Return(v)),
                    }
                    let cond = self.eval_expr_with_output(condition, output)?;
                    if !cond.is_truthy() {
                        break;
                    }
                }
                Ok(StmtResult::Normal)
            }

            Stmt::For {
                init,
                condition,
                update,
                body,
                ..
            } => {
                // Execute init
                if let Some(init_stmt) = init {
                    self.execute_stmt(init_stmt, output)?;
                }

                while !(self.should_exit || self.should_next || self.should_nextfile) {
                    // Check condition
                    if let Some(cond_expr) = condition {
                        let cond = self.eval_expr_with_output(cond_expr, output)?;
                        if !cond.is_truthy() {
                            break;
                        }
                    }

                    // Execute body
                    match self.execute_stmt(body, output)? {
                        StmtResult::Normal | StmtResult::Continue => {}
                        StmtResult::Break => break,
                        StmtResult::Return(v) => return Ok(StmtResult::Return(v)),
                    }

                    // Execute update
                    if let Some(update_expr) = update {
                        self.eval_expr_with_output(update_expr, output)?;
                    }
                }
                Ok(StmtResult::Normal)
            }

            Stmt::ForIn {
                var,
                array,
                parents,
                body,
                ..
            } => {
                let path = self.array_path(parents)?;
                let keys: Vec<String> = self.table(array, &path)?.keys().cloned().collect();
                for key in keys {
                    if self.should_exit || self.should_next || self.should_nextfile {
                        break;
                    }
                    if self.arrays.contains_key(self.resolve_array_name(var)) {
                        return Err(crate::Error::runtime("array used as loop index"));
                    }
                    self.set_variable_value(var, Value::from_string(key))?;
                    match self.execute_stmt(body, output)? {
                        StmtResult::Normal | StmtResult::Continue => continue,
                        StmtResult::Break => break,
                        StmtResult::Return(v) => return Ok(StmtResult::Return(v)),
                    }
                }
                Ok(StmtResult::Normal)
            }

            Stmt::Block(block) => self.execute_block(block, output),

            Stmt::Break { .. } => Ok(StmtResult::Break),

            Stmt::Continue { .. } => Ok(StmtResult::Continue),

            Stmt::Next { .. } => {
                self.should_next = true;
                Ok(StmtResult::Normal)
            }

            Stmt::Nextfile { .. } => {
                self.should_nextfile = true;
                Ok(StmtResult::Normal)
            }

            Stmt::Exit { code, .. } => {
                self.exit_code = code
                    .as_ref()
                    .map(|e| {
                        self.eval_expr_with_output(e, output)
                            .map(|v| v.to_number() as i32)
                    })
                    .transpose()?
                    .unwrap_or(0);
                self.should_exit = true;
                Ok(StmtResult::Normal)
            }

            Stmt::Return { value, .. } => {
                let val = value
                    .as_ref()
                    .map(|e| self.eval_expr_with_output(e, output))
                    .transpose()?
                    .unwrap_or(Value::Uninitialized);
                Ok(StmtResult::Return(val))
            }

            Stmt::Delete {
                array,
                parents,
                index,
                ..
            } => {
                if index.is_empty() {
                    // delete array (entire array)
                    self.arrays.remove(array);
                } else {
                    let key_parts: Result<Vec<Value>> = index
                        .iter()
                        .map(|e| self.eval_expr_with_output(e, output))
                        .collect();
                    let key = self.make_array_key(&key_parts?);
                    let path = self.array_path(parents)?;
                    let table = self.table(array, &path)?;
                    table.scalars.remove(&key);
                    table.children.remove(&key);
                }
                Ok(StmtResult::Normal)
            }
        }
    }

    fn execute_print<W: Write>(
        &mut self,
        args: &[Expr],
        redirect: &Option<OutputRedirect>,
        default_output: &mut W,
    ) -> Result<()> {
        let values: Result<Vec<String>> = args
            .iter()
            .map(|e| {
                let v = self.eval_expr_with_output(e, default_output)?;
                Ok(self.to_output_str(&v))
            })
            .collect();
        let values = values?;

        let line = if values.is_empty() {
            // print without args prints $0
            self.record_for_output().into_owned()
        } else {
            values.join(&self.ofs)
        };
        let line = line + &self.ors;

        self.output_text(&line, redirect, default_output)
    }

    fn execute_printf<W: Write>(
        &mut self,
        format_expr: &Expr,
        args: &[Expr],
        redirect: &Option<OutputRedirect>,
        default_output: &mut W,
    ) -> Result<()> {
        let format = self
            .eval_expr_with_output(format_expr, default_output)?
            .to_string_val();
        let values: Result<Vec<Value>> = args
            .iter()
            .map(|e| self.eval_expr_with_output(e, default_output))
            .collect();
        let values = values?;

        let formatted = self.format_printf(&format, &values);

        self.output_text(&formatted, redirect, default_output)
    }

    fn output_text<W: Write>(
        &mut self,
        text: &str,
        redirect: &Option<OutputRedirect>,
        default: &mut W,
    ) -> Result<()> {
        let Some(target) = redirect else {
            return crate::records::emit(default, text, self.bytes);
        };
        self.allow_redirect()?;
        let (expr, append, pipe) = match target {
            OutputRedirect::Truncate(e) => (e, false, false),
            OutputRedirect::Append(e) => (e, true, false),
            OutputRedirect::Pipe(e) => (e, false, true),
        };
        let name = self.eval_expr_with_output(expr, default)?.to_string_val();
        if !self.output_files.contains_key(&(name.clone(), pipe)) {
            let file = self.host.output(&name, append, pipe)?;
            self.output_files.insert((name.clone(), pipe), file);
        }
        crate::records::emit(
            self.output_files.get_mut(&(name, pipe)).unwrap(),
            text,
            self.bytes,
        )
    }

    pub(crate) fn format_printf(&self, format: &str, args: &[Value]) -> String {
        crate::fmt::sprintf(format, args)
    }
}
