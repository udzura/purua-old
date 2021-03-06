use crate::parser::*;
use crate::state::*;
use crate::value::Value;

use log::debug;

macro_rules! is_exact_rule1 {
    ($rule:path, $y:expr) => {
        match $y {
            $rule(val) => Ok(val),
            _ => Err(LuaError {
                message: format!("Invalid rule passed: {:?}", $y),
            }),
        }
    };
}

macro_rules! is_exact_rule2 {
    ($rule:path, $y:expr) => {
        match $y {
            $rule(val1, val2) => Ok((val1, val2)),
            _ => Err(LuaError {
                message: format!("Invalid rule passed: {:?}", $y),
            }),
        }
    };
}

pub fn eval_exp(l: &mut LuaState, exp: &Rule) -> Result<Value, LuaError> {
    let exp_: &Box<Rule> = is_exact_rule1!(Rule::Exp, exp)?;
    let exp_ = exp_.as_ref();
    match exp_ {
        Rule::Nil => return Ok(Value::Nil),
        Rule::Bool(b) => return Ok(Value::Bool(b.to_owned())),
        Rule::Numeral(n) => return Ok(Value::Number(n.to_owned() as i64)),
        Rule::LiteralString(s) => return Ok(Value::LuaString(s.to_string())),
        Rule::Prefixexp(_) => eval_prefixexp(l, exp_),
        Rule::TableConst(_) => eval_tableconst(l, exp_),
        Rule::BinOp(_, _, _) => eval_binop(l, exp_),
        Rule::UnOp(_, _) => eval_unop(l, exp_),
        _ => Err(LuaError {
            message: format!("Unsupported exp rule: {:?}", exp_),
        }),
    }
}

pub fn eval_binop(l: &mut LuaState, binop: &Rule) -> Result<Value, LuaError> {
    match binop {
        Rule::BinOp(c, lhs, rhs) => {
            let lhs = lhs.as_ref();
            let lvalue = match lhs {
                Rule::Exp(_) => eval_exp(l, lhs)?,
                Rule::BinOp(_, _, _) => eval_binop(l, lhs)?,
                Rule::UnOp(_, _) => eval_unop(l, lhs)?,
                _ => {
                    return Err(l.error("lhs invalid"));
                }
            };
            let rhs = rhs.as_ref();
            let rvalue = match rhs {
                Rule::Exp(_) => eval_exp(l, rhs)?,
                Rule::BinOp(_, _, _) => eval_binop(l, rhs)?,
                Rule::UnOp(_, _) => eval_unop(l, rhs)?,
                _ => {
                    return Err(l.error("rhs invalid"));
                }
            };

            l.process_op(c, lvalue, rvalue)
        }
        _ => Err(l.error("binop invalid")),
    }
}

pub fn eval_unop(l: &mut LuaState, unop: &Rule) -> Result<Value, LuaError> {
    match unop {
        Rule::UnOp(c, exp) => {
            let exp = exp.as_ref();
            let value = match exp {
                Rule::Exp(_) => eval_exp(l, exp)?,
                Rule::BinOp(_, _, _) => eval_binop(l, exp)?,
                Rule::UnOp(_, _) => eval_unop(l, exp)?,
                _ => {
                    return Err(l.error("unop exp invalid"));
                }
            };

            l.process_unop(c, value)
        }
        _ => Err(l.error("unop invalid")),
    }
}

pub fn eval_get_var(l: &mut LuaState, exp: &Rule) -> Result<Value, LuaError> {
    let var = is_exact_rule1!(Rule::Var, exp)?;
    let name = is_exact_rule1!(Rule::Symbol, var.as_ref())?;

    l.get_local(name)
        .or(l.get_global(name))
        .ok_or(l.error("Variable not found"))
}

pub fn eval_prefixexp(l: &mut LuaState, pexp: &Rule) -> Result<Value, LuaError> {
    let value: &Box<Rule> = is_exact_rule1!(Rule::Prefixexp, pexp)?;
    let value = value.as_ref();
    match value {
        Rule::FunctionCall(_, _) => eval_funcall(l, value),
        Rule::Var(_) => eval_get_var(l, value),
        Rule::Exp(_) => eval_exp(l, value),
        _ => Err(LuaError {
            message: format!("Unsupported rule: {:?}", value),
        }),
    }
}

pub fn eval_tableconst(l: &mut LuaState, exp: &Rule) -> Result<Value, LuaError> {
    let list: &Box<Rule> = is_exact_rule1!(Rule::TableConst, exp)?;
    let list: &Vec<Box<Rule>> = is_exact_rule1!(Rule::FieldList, list.as_ref())?;

    let v = Value::newtable();
    let t = v.ensure_table()?;

    for field in list.iter() {
        let (key, value) = is_exact_rule2!(Rule::Field, field.as_ref())?;
        match key.as_ref() {
            Rule::Symbol(_n) => {
                unimplemented!("TODO: table");
            }
            Rule::Nop => {
                let mut t = t.vec.borrow_mut();
                t.push(eval_exp(l, value.as_ref())?);
            }
            _ => {
                return Err(LuaError {
                    message: format!("Unsupported rule for field key: {:?}", value),
                });
            }
        }
    }

    Ok(v)
}

pub fn eval_funcall(l: &mut LuaState, fc: &Rule) -> Result<Value, LuaError> {
    let (name, args) = is_exact_rule2!(Rule::FunctionCall, fc)?;
    let name = is_exact_rule1!(Rule::Symbol, name.as_ref())?;
    let exp = is_exact_rule1!(Rule::Args, args.as_ref())?.as_ref();
    match exp {
        Rule::Exp(_) => {
            let arg1v = eval_exp(l, exp)?;
            debug!("get param {} {:?}", name, &arg1v);
            let ret = l.global_funcall1(name, arg1v)?;
            Ok(ret)
        }
        Rule::Nop => {
            let ret = l.global_funcall1(name, Value::Nil)?;
            Ok(ret)
        }
        _ => Err(l.error("Invalid rule")),
    }
}

pub fn eval_funcall_multi(l: &mut LuaState, fc: &Rule) -> Result<Vec<Value>, LuaError> {
    let (name, args) = is_exact_rule2!(Rule::FunctionCall, fc)?;
    let name = is_exact_rule1!(Rule::Symbol, name.as_ref())?;
    let exp = is_exact_rule1!(Rule::Args, args.as_ref())?.as_ref();

    let func = l
        .get_global(name)
        .ok_or(l.error("Please specify func name"))?;

    match exp {
        Rule::Exp(_) => {
            let arg1v = eval_exp(l, exp)?;
            let ret = l.funcall(func, vec![arg1v])?;
            Ok(ret)
        }
        Rule::Nop => {
            let ret = l.funcall(func, vec![])?;
            Ok(ret)
        }
        _ => Err(l.error("Invalid rule")),
    }
}

pub fn eval_ifthen(l: &mut LuaState, stat: &Rule) -> Result<Value, LuaError> {
    let (exps, blocks) = is_exact_rule2!(Rule::IfStat, stat)?;
    let mut i = 0;
    for exp in exps.iter() {
        i += 1;
        let exp = exp.as_ref();
        match exp {
            Rule::Exp(_) => {
                let v = eval_exp(l, exp)?;
                match v {
                    Value::Nil => {
                        continue;
                    }
                    Value::Bool(b) => {
                        if b {
                            return eval_block(l, &blocks[i - 1]);
                        } else {
                            continue;
                        }
                    }
                    _ => {
                        return eval_block(l, &blocks[i - 1]);
                    }
                }
            }
            Rule::Nop => {
                return eval_block(l, &blocks[i - 1]);
            }
            _ => return Err(l.error("Invalid rule")),
        }
    }
    Ok(Value::Nil)
}

pub fn process_funcname(_l: &mut LuaState, fname: &Rule) -> Result<String, LuaError> {
    let name = is_exact_rule1!(Rule::FuncName, fname)?;
    let name = is_exact_rule1!(Rule::Symbol, name.as_ref())?;
    Ok(name.to_string())
}

pub fn process_params(_l: &mut LuaState, params: &Rule) -> Result<Vec<String>, LuaError> {
    let name = is_exact_rule1!(Rule::ParList1, params)?;
    let name = is_exact_rule1!(Rule::Symbol, name.as_ref())?;
    Ok(vec![name.to_string()])
}

pub fn eval_funcbody<'a>(
    l: &mut LuaState,
    fb: &'a Rule,
) -> Result<(Vec<String>, &'a Rule), LuaError> {
    if let Rule::FuncBody(params, body) = fb {
        let body = body.as_ref();
        if let Rule::Block(_) = body {
            let params = if params.is_some() {
                process_params(l, params.as_ref().unwrap())?
            } else {
                vec![]
            };
            return Ok((params, body));
        }
    }
    Err(l.error("Invalid composite of funcbody"))
}

pub fn eval_chunk(l: &mut LuaState, chunk: &Rule) -> Result<Value, LuaError> {
    match chunk {
        Rule::Chunk(stats, last) => {
            for stat in stats.into_iter() {
                let ret = eval_stat(l, stat.as_ref())?;
                if l.to_return() {
                    return Ok(ret);
                }
            }
            if let Some(stat) = last {
                let exp = is_exact_rule1!(Rule::LastStat, stat.as_ref())?;
                let ret = eval_exp(l, exp.as_ref())?;
                if let Some(_) = l.current_frame() {
                    l.set_to_return(true);
                }
                Ok(ret)
            } else {
                Ok(Value::Nil)
            }
        }
        _ => Err(l.error("Not a chunk")),
    }
}

pub fn eval_stat(l: &mut LuaState, stat: &Rule) -> Result<Value, LuaError> {
    match stat {
        Rule::Stat(kind, a, b, c, _d, _e) => {
            let v = match kind {
                StatKind::Sep => Value::Nil,
                StatKind::VarAssign => {
                    let var = is_exact_rule1!(Rule::Var, a.as_ref().unwrap().as_ref())?;
                    let name = is_exact_rule1!(Rule::Symbol, var.as_ref())?;
                    let value = eval_exp(l, b.as_ref().unwrap())?;

                    if l.has_local_name(name) {
                        l.assign_local(name, value);
                    } else {
                        l.assign_global(name, value);
                    }
                    Value::Nil
                }
                StatKind::FunctionCall => eval_funcall(l, a.as_ref().unwrap())?,
                StatKind::DeclareFunction => {
                    let name = process_funcname(l, a.as_ref().unwrap())?;
                    let (params, block) = eval_funcbody(l, b.as_ref().unwrap())?;

                    l.register_global_code(name, params, block);
                    Value::Nil
                }
                StatKind::IfThen => eval_ifthen(l, a.as_ref().unwrap())?,
                StatKind::LocalVar => {
                    let name = is_exact_rule1!(Rule::Symbol, a.as_ref().unwrap().as_ref())?;
                    let exp = b.as_ref().unwrap().as_ref();
                    let value = match exp {
                        Rule::Exp(_) => eval_exp(l, exp)?,
                        _ => {
                            return Err(l.error("Expected exp"));
                        }
                    };
                    if l.current_frame().is_some() {
                        l.assign_local(name, value);
                    } else {
                        return Err(l.error("Expected in function def"));
                    }
                    Value::Nil
                }
                StatKind::ForIn => {
                    let vars = is_exact_rule1!(Rule::SymbolList, a.as_ref().unwrap().as_ref())?;
                    let funex = is_exact_rule1!(Rule::Exp, b.as_ref().unwrap().as_ref())?;
                    let fc = is_exact_rule1!(Rule::Prefixexp, funex.as_ref())?;
                    let mut loop_params = eval_funcall_multi(l, fc)?;
                    let mut key = loop_params.pop().unwrap();
                    let collction = loop_params.pop().unwrap();
                    let next = loop_params.pop().unwrap();
                    loop {
                        let mut values =
                            l.funcall(next.clone(), vec![collction.clone(), key.clone()])?;
                        match values[0] {
                            Value::Nil => {
                                break;
                            }
                            _ => {}
                        }
                        key = values[0].to_owned();
                        let oldtop = l.start_block_raw();
                        for name in vars.iter().rev() {
                            let name = is_exact_rule1!(Rule::Symbol, name.as_ref())?;
                            l.assign_local(name, values.pop().unwrap());
                        }
                        eval_block(l, c.as_ref().unwrap().as_ref())?;
                        l.end_block_raw(oldtop)?;
                    }
                    Value::Nil
                }
                _ => unimplemented!("{:?}: Pull request is welcomed!", kind),
            };
            Ok(v)
        }
        _ => Err(l.error("Not a stat")),
    }
}

pub fn eval_block(l: &mut LuaState, block: &Rule) -> Result<Value, LuaError> {
    let chunk = is_exact_rule1!(Rule::Block, block)?;
    eval_chunk(l, chunk)
}
