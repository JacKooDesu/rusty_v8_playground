use v8::{self, ContextOptions, HandleScope};

#[derive(Debug)]
struct Point {
    x: i32,
    y: i32,
}

impl Point {
    fn move_by(&mut self, dx: i32, dy: i32) {
        self.x += dx;
        self.y += dy;
    }
}

fn point_constructor(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    mut _rv: v8::ReturnValue,
) {
    let this = args.this();
    let x = args.get(0).int32_value(scope).unwrap_or(0);
    let y = args.get(1).int32_value(scope).unwrap_or(0);
    let point = Box::new(Point { x, y });
    let external = v8::External::new(scope, Box::into_raw(point) as *mut std::ffi::c_void);
    this.set_internal_field(0, external.into());
}

fn point_move_by(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let this = args.this();
    let external =
        v8::Local::<v8::External>::try_from(this.get_internal_field(scope, 0).unwrap()).unwrap();
    let point = unsafe { &mut *(external.value() as *mut Point) };

    let dx = args.get(0).int32_value(scope).unwrap_or(0);
    let dy = args.get(1).int32_value(scope).unwrap_or(0);
    point.move_by(dx, dy);

    let arr = v8::Array::new(scope, 2);
    let x_val = v8::Integer::new(scope, point.x);
    arr.set_index(scope, 0, x_val.into());
    let y_val = v8::Integer::new(scope, point.y);
    arr.set_index(scope, 1, y_val.into());
    rv.set(arr.into());
}

fn test_call_js_function(
    context: v8::Local<v8::Context>,
    context_scope: &mut v8::ContextScope<HandleScope>,
) {
    let fn_str = v8::String::new(context_scope, "Test").unwrap();
    let func = context
        .global(context_scope)
        .get(context_scope, fn_str.into())
        .unwrap();
    let func = v8::Local::<v8::Function>::try_from(func).unwrap();

    let recv = v8::undefined(context_scope);
    let point_ctor = context
        .global(context_scope)
        .get(
            context_scope,
            v8::String::new(context_scope, "Point").unwrap().into(),
        )
        .map(|x| v8::Local::<v8::Function>::try_from(x).unwrap())
        .unwrap();
    let obj = point_ctor
        .new_instance(
            context_scope,
            &[
                v8::Integer::new(context_scope, 0).into(),
                v8::Integer::new(context_scope, 1).into(),
            ],
        )
        .unwrap();

    let result = func.call(context_scope, recv.into(), &[obj.into()]);

    if let Some(obj) = result.and_then(|x| x.to_object(context_scope)) {
        let external =
            v8::Local::<v8::External>::try_from(obj.get_internal_field(context_scope, 0).unwrap())
                .unwrap();
        let point_ptr = external.value() as *mut Point;
        let point_ref = unsafe { &*point_ptr };
        println!("x: {}", point_ref.x);
        println!("y: {}", point_ref.y);
    }
}

fn main() {
    let platform = v8::new_default_platform(0, false).make_shared();
    v8::V8::initialize_platform(platform);
    v8::V8::initialize();

    {
        let isolate = &mut v8::Isolate::new(Default::default());

        v8::scope!(let handle_scope, isolate);
        let context = v8::Context::new(handle_scope, ContextOptions::default());
        let scope = &mut v8::ContextScope::new(handle_scope, context);

        // 新增 Point 類別 Template，包含 moveBy 方法
        let ctor = v8::FunctionTemplate::new(scope, point_constructor);
        ctor.set_class_name(v8::String::new(scope, "Point").unwrap());
        ctor.instance_template(scope).set_internal_field_count(1);
        ctor.prototype_template(scope).set(
            v8::String::new(scope, "moveBy").unwrap().into(),
            v8::FunctionTemplate::new(scope, point_move_by).into(),
        );

        // 註冊 Point Template 到全域
        let global = context.global(scope);
        global.set(
            scope,
            v8::String::new(scope, "Point").unwrap().into(),
            ctor.get_function(scope).unwrap().into(),
        );

        let code_str = std::fs::read_to_string("script.js").expect("cannot resd script.js");

        let code = v8::String::new(scope, code_str.as_str()).unwrap();
        let script = v8::Script::compile(scope, code, None).unwrap();
        let result = script.run(scope).unwrap();

        if result.is_object() {
            let obj = result.to_object(scope).unwrap();
            let external =
                v8::Local::<v8::External>::try_from(obj.get_internal_field(scope, 0).unwrap())
                    .unwrap();
            let point_ptr = external.value() as *mut Point;
            let point_ref = unsafe { &*point_ptr };
            println!("x: {}", point_ref.x);
            println!("y: {}", point_ref.y);
        }

        test_call_js_function(context, scope);
    }
    unsafe {
        v8::V8::dispose();
    }
    v8::V8::dispose_platform();
}
