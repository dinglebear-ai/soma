pub use rquickjs as quickjs;

use std::str;

use anyhow::{bail, Error, Result};
use quickjs::{
    convert, prelude::Rest, Context, Ctx, Error as JsError, Exception, FromJs,
    Runtime as QuickJsRuntime, String as JsString, Value,
};

pub struct Config {
    memory_limit: usize,
    max_stack_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            memory_limit: usize::MAX,
            max_stack_size: 256 * 1024,
        }
    }
}

impl Config {
    pub fn redirect_stdout_to_stderr(&mut self, _enable: bool) -> &mut Self {
        self
    }

    pub fn memory_limit(&mut self, bytes: usize) -> &mut Self {
        self.memory_limit = bytes;
        self
    }

    pub fn max_stack_size(&mut self, bytes: usize) -> &mut Self {
        self.max_stack_size = bytes;
        self
    }
}

pub struct Runtime {
    inner: QuickJsRuntime,
    context: Context,
}

impl Runtime {
    pub fn new(config: Config) -> Result<Self> {
        let inner = QuickJsRuntime::new()?;
        inner.set_memory_limit(config.memory_limit);
        inner.set_max_stack_size(config.max_stack_size);
        let context = Context::full(&inner)?;
        Ok(Self { inner, context })
    }

    pub fn context(&self) -> &Context {
        &self.context
    }

    pub fn resolve_pending_jobs(&self) -> Result<()> {
        while self.inner.is_job_pending() {
            match self.inner.execute_pending_job() {
                Ok(true) => {}
                Ok(false) => break,
                Err(err) => bail!("{err}"),
            }
        }
        Ok(())
    }
}

pub struct Args<'js>(Ctx<'js>, Rest<Value<'js>>);

impl<'js> Args<'js> {
    pub fn hold(cx: Ctx<'js>, args: Rest<Value<'js>>) -> Self {
        Self(cx, args)
    }

    pub fn release(self) -> (Ctx<'js>, Rest<Value<'js>>) {
        (self.0, self.1)
    }
}

pub fn to_js_error(cx: Ctx<'_>, error: Error) -> JsError {
    match error.downcast::<JsError>() {
        Ok(error) => error,
        Err(error) => cx.throw(Value::from_exception(
            Exception::from_message(cx.clone(), &error.to_string())
                .expect("creating JS exception should succeed"),
        )),
    }
}

pub fn val_to_string<'js>(cx: &Ctx<'js>, value: Value<'js>) -> Result<String> {
    if let Some(symbol) = value.as_symbol() {
        if let Some(description) = symbol.description()?.into_string() {
            let description = description
                .to_string()
                .unwrap_or_else(|err| to_string_lossy(cx, &description, err));
            Ok(format!("Symbol({description})"))
        } else {
            Ok("Symbol()".to_string())
        }
    } else {
        let stringified = <convert::Coerced<JsString<'js>>>::from_js(cx, value).map(|string| {
            string
                .to_string()
                .unwrap_or_else(|err| to_string_lossy(cx, &string.0, err))
        })?;
        Ok(stringified)
    }
}

fn to_string_lossy<'js>(cx: &Ctx<'js>, string: &JsString<'js>, error: JsError) -> String {
    let mut len: quickjs::qjs::size_t = 0;
    let ptr = unsafe {
        quickjs::qjs::JS_ToCStringLen2(cx.as_raw().as_ptr(), &mut len, string.as_raw(), false)
    };
    let mut buffer = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    let mut utf8_error = match error {
        JsError::Utf8(error) => error,
        _ => return String::new(),
    };
    let mut result = String::new();
    loop {
        let (valid, after_valid) = buffer.split_at(utf8_error.valid_up_to());
        result.push_str(unsafe { str::from_utf8_unchecked(valid) });
        result.push(char::REPLACEMENT_CHARACTER);
        let lone_surrogate = matches!(after_valid, [0xED, 0xA0..=0xBF, 0x80..=0xBF, ..]);
        let error_len = if lone_surrogate {
            3
        } else {
            utf8_error.error_len().unwrap_or(1)
        };
        buffer = &after_valid[error_len..];
        match str::from_utf8(buffer) {
            Ok(valid) => {
                result.push_str(valid);
                break;
            }
            Err(error) => utf8_error = error,
        }
    }
    result
}
