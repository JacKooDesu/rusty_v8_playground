# Rust ↔ JS/V8/TypeScript 綁定討論整理

---

## 1. rusty_v8 的基本用法

### main.rs 範例說明

```rust
use rusty_v8 as v8;

fn main() {
    let platform = v8::new_default_platform(0, false).make_shared();
    v8::V8::initialize_platform(platform);
    v8::V8::initialize();

    let isolate = &mut v8::Isolate::new(Default::default());
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);

    let code = v8::String::new(scope, "1 + 2").unwrap();
    let script = v8::Script::compile(scope, code, None).unwrap();
    let result = script.run(scope).unwrap();
    let result = result.to_string(scope).unwrap();
    println!("{}", result.to_rust_string_lossy(scope));
}
```

**說明：**
- 初始化 V8 執行平台
- 建立 Isolate、HandleScope、Context、ContextScope
- 編譯並執行 JS 程式碼，印出結果

---

## 2. HandleScope 與 ContextScope 的差異

| HandleScope                        | ContextScope                          |
|------------------------------------|---------------------------------------|
| 管理 V8 物件的生命週期              | 指定 JS 執行的上下文（全域環境）        |
| 控制物件何時被釋放                  | 控制程式碼在哪個 Context 下執行         |
| 主要解決記憶體管理問題              | 主要解決多個 JS 環境切換問題            |

**巢狀關係圖與範例：**

```
Isolate
  └── HandleScope
        └── Context
              └── ContextScope
                    └── (執行 JS 程式碼)
```

```rust
let isolate = &mut v8::Isolate::new(Default::default()); // Isolate
let scope = &mut v8::HandleScope::new(isolate);         // HandleScope
let context = v8::Context::new(scope);                  // Context
let scope = &mut v8::ContextScope::new(scope, context); // ContextScope
// 在這裡執行 JS 程式碼
```

---

## 3. JS ↔ Rust 對象與方法綁定

### JS 呼叫 Rust 函式

```rust
fn add_callback(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let a = args.get(0).integer_value(scope).unwrap_or(0);
    let b = args.get(1).integer_value(scope).unwrap_or(0);
    let sum = a + b;
    let result = v8::Integer::new(scope, sum);
    rv.set(result.into());
}

// 綁定到 JS
let global = context.global(scope);
let fn_template = v8::FunctionTemplate::new(scope, add_callback);
let function = fn_template.get_function(scope).unwrap();
let key = v8::String::new(scope, "add").unwrap();
global.set(scope, key.into(), function.into());
```

JS 端呼叫：
```js
let result = add(3, 4); // result 會是 7
```

### JS 操作 Rust struct

```rust
struct Point {
    x: i32,
    y: i32,
}

// 先建立 ObjectTemplate 並設 internal field count
// let tmpl = v8::ObjectTemplate::new(scope);
// tmpl.set_internal_field_count(1);

fn point_constructor(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let x = args.get(0).integer_value(scope).unwrap_or(0);
    let y = args.get(1).integer_value(scope).unwrap_or(0);
    let point = Box::new(Point { x, y });
    let raw_ptr = Box::into_raw(point) as *mut std::ffi::c_void;
    let external = v8::External::new(scope, raw_ptr);

    // 用 ObjectTemplate 產生 JS 物件
    // 假設 tmpl 已設 internal field count
    let tmpl = v8::ObjectTemplate::new(scope);
    tmpl.set_internal_field_count(1);
    let obj = tmpl.new_instance(scope).unwrap();
    obj.set_internal_field(0, external.into());

    // 將 obj 回傳給 JS
    rv.set(obj.into());
}
```

### JS 呼叫 Rust struct 方法（方法綁定與 JS 端呼叫範例）

```rust
impl Point {
    fn move_by(&mut self, dx: i32, dy: i32) {
        self.x += dx;
        self.y += dy;
    }
}

// 綁定 Rust 方法到 JS 原型
fn point_move_by(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let this = args.this();
    let external = v8::Local::<v8::External>::try_from(
        this.get_internal_field(0).unwrap()
    ).unwrap();
    let point = unsafe { &mut *(external.value() as *mut Point) };

    let dx = args.get(0).integer_value(scope).unwrap_or(0);
    let dy = args.get(1).integer_value(scope).unwrap_or(0);

    point.move_by(dx, dy);
    // 回傳新座標
    let arr = v8::Array::new(scope, 2);
    let x_val = v8::Integer::new(scope, point.x);
    arr.set_index(scope, 0, x_val.into());
    let y_val = v8::Integer::new(scope, point.y);
    arr.set_index(scope, 1, y_val.into());
    rv.set(arr.into());
}

// 綁定到 FunctionTemplate 原型
let tmpl = v8::ObjectTemplate::new(scope);
tmpl.set_internal_field_count(1);

let ctor = v8::FunctionTemplate::new(scope, point_constructor);
ctor.set_class_name(v8::String::new(scope, "Point").unwrap());
// 不再有 instance_template()，internal field count 已在 ObjectTemplate 設定

let move_by_tmpl = v8::FunctionTemplate::new(scope, point_move_by);
ctor.prototype_template().set(
    v8::String::new(scope, "moveBy").unwrap().into(),
    move_by_tmpl.into(),
);

// 註冊到 global
let context = ...; // 你的 v8::Context
let global = context.global(scope);
global.set(
    scope,
    v8::String::new(scope, "Point").unwrap().into(),
    ctor.get_function(scope).unwrap().into(),
);
```

JS 端呼叫：
```js
let p = new Point(10, 20);
p.moveBy(5, -3); // p.x = 15, p.y = 17
```

---

## 4. 記憶體管理（GC 時釋放 Rust 物件）

- 用 Box::into_raw 交 pointer 給 JS
- 用 v8::Weak<T>::with_finalizer 註冊 finalizer，GC 時釋放 Box
- 只釋放用 Box::into_raw 交出去的 pointer

### 範例：Rust + v8::Weak<T> 釋放記憶體（2024 新寫法）

```rust
use v8::{HandleScope, Isolate, Local, Object, Weak};

struct Point {
    x: i32,
    y: i32,
}

// 建立 JS 物件並存放 Rust pointer
fn point_constructor(
    isolate: &mut Isolate,
    scope: &mut HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let point = Box::new(Point { x: 0, y: 0 });
    let raw_ptr = Box::into_raw(point);

    // 建立 JS 物件
    let obj = v8::Object::new(scope);

    // 設定 internal field（新版 API）
    // 先確保 ObjectTemplate 有 internal field
    // 例如在建立 ObjectTemplate 時:
    // tmpl.set_internal_field_count(1);
    // 這裡假設 obj 來自該 template

    let external = v8::External::new(scope, raw_ptr as *mut std::ffi::c_void);
    obj.set_internal_field(0, external.into());

    // 建立 Weak handle，註冊 finalizer
    let _weak = Weak::with_finalizer(isolate, obj, Box::new(move |_isolate| {
        // GC 時釋放記憶體
        unsafe { let _ = Box::from_raw(raw_ptr); }
    }));

    // 你可以選擇將 weak handle 存在某個結構或關聯到 JS 物件
}
```

**說明：**
- 用 `Box::into_raw` 產生裸指標，存到 JS 物件 internal field
- 用 `v8::Weak::with_finalizer` 註冊 finalizer，當 JS 物件被 GC 時自動釋放 Rust 端記憶體
- finalizer 不能存取 JS 物件內容（因為 JS 物件已被 GC）
- 這樣可避免記憶體洩漏，且是目前 rusty_v8/v8 crate 官方推薦的做法

---

## 5. JS 物件回傳給 Rust

- Rust 端可取得 v8::Local<v8::Object>，查詢/設定屬性、呼叫方法
- 不能直接轉型成 Rust struct，但可讀取資料後建立 Rust struct

```rust
let code = v8::String::new(scope, "makeObj()").unwrap();
let script = v8::Script::compile(scope, code, None).unwrap();
let result = script.run(scope).unwrap();

if result.is_object() {
    let obj = result.to_object(scope).unwrap();
    // 查詢屬性
    let x = obj.get(scope, v8::String::new(scope, "x").unwrap().into()).unwrap();
    let x_val = x.integer_value(scope).unwrap();
    // 呼叫方法
    let say_hi = obj.get(scope, v8::String::new(scope, "sayHi").unwrap().into()).unwrap();
    let func = v8::Local::<v8::Function>::try_from(say_hi).unwrap();
    let res = func.call(scope, obj.into(), &[]).unwrap();
    let hi_str = res.to_string(scope).unwrap().to_rust_string_lossy(scope);
    println!("x = {}, sayHi() = {}", x_val, hi_str);
}
```

---

## 6. JS 回傳 Rust struct 的情境

- JS 端回傳的其實是包著 Rust pointer 的 JS 物件
- Rust 端可用 internal field 取回 pointer，操作原生 struct

```rust
let code = v8::String::new(scope, "makePoint()").unwrap();
let script = v8::Script::compile(scope, code, None).unwrap();
let result = script.run(scope).unwrap();

if result.is_object() {
    let obj = result.to_object(scope).unwrap();
    // 取出 internal field
    let external = v8::Local::<v8::External>::try_from(obj.get_internal_field(scope, 0).unwrap()).unwrap();
    let point_ptr = external.value() as *mut Point;
    let point_ref = unsafe { &*point_ptr };
    println!("Point from JS: x={}, y={}", point_ref.x, point_ref.y);
}
```

---

## 7. 操作容器類型（如 Vec）

- JS 端持有 Rust Vec 的 wrapper
- 綁定 push/get/len 等方法
- 記憶體釋放同 struct

```rust
struct MyVec {
    data: Vec<i32>,
}

// 先建立 ObjectTemplate 並設 internal field count
// let tmpl = v8::ObjectTemplate::new(scope);
// tmpl.set_internal_field_count(1);

fn myvec_constructor(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let vec = Box::new(MyVec { data: Vec::new() });
    let raw_ptr = Box::into_raw(vec) as *mut std::ffi::c_void;
    let external = v8::External::new(scope, raw_ptr);

    // 用 ObjectTemplate 產生 JS 物件
    let tmpl = v8::ObjectTemplate::new(scope);
    tmpl.set_internal_field_count(1);
    let obj = tmpl.new_instance(scope).unwrap();
    obj.set_internal_field(0, external.into());
    rv.set(obj.into());
}

fn myvec_push(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let this = args.this();
    let external = v8::Local::<v8::External>::try_from(this.get_internal_field(0).unwrap()).unwrap();
    let vec = unsafe { &mut *(external.value() as *mut MyVec) };
    let val = args.get(0).integer_value(scope).unwrap_or(0);
    vec.data.push(val);
}

fn myvec_get(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let this = args.this();
    let external = v8::Local::<v8::External>::try_from(this.get_internal_field(0).unwrap()).unwrap();
    let vec = unsafe { &*(external.value() as *const MyVec) };
    let idx = args.get(0).integer_value(scope).unwrap_or(0) as usize;
    if let Some(&v) = vec.data.get(idx) {
        rv.set(v8::Integer::new(scope, v).into());
    }
}

fn myvec_len(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let this = args.this();
    let external = v8::Local::<v8::External>::try_from(this.get_internal_field(0).unwrap()).unwrap();
    let vec = unsafe { &*(external.value() as *const MyVec) };
    rv.set(v8::Integer::new(scope, vec.data.len() as i32).into());
}
```

JS 端呼叫：
```js
let v = new MyVec();
v.push(10);
v.push(20);
console.log(v.get(0)); // 10
console.log(v.len());  // 2
```

---

## 8. 綁定資訊自動產生 TypeScript 型別定義（index.d.ts）

### 推薦設計流程

1. Rust 端用巨集/屬性標註要導出的類型/方法
2. 用 procedural macro 或 build script 產生綁定描述（JSON/metadata）
3. 用腳本根據描述產生 index.d.ts
4. 維護同步

---

## 9. 巨集範例

```rust
#[js_class]
pub struct MyVec {
    data: Vec<i32>,
}

impl MyVec {
    #[js_method]
    pub fn push(&mut self, v: i32) {}
    #[js_method]
    pub fn get(&self, idx: usize) -> i32 { 0 }
    #[js_method]
    pub fn len(&self) -> usize { 0 }
}
```

---

## 10. tslink crate 適用性分析

**tslink** 是一個專門用來將 Rust 類型自動轉成 TypeScript 型別定義的 crate，特點如下：

- 支援 struct、enum、方法、async、callback、錯誤處理等
- 可自訂 enum 映射、命名規則、忽略欄位、重新命名、指定輸出目錄
- 可搭配 node-bindgen 產生 npm package
- 產生的 TypeScript 型別貼近 Rust 原設計
- 只需加上 #[tslink]、#[tslink(class)] 等屬性即可

**結論：**
- 如果你要 Rust 綁定自動產生 TypeScript 型別定義，tslink 非常合適！

---

## 11. tslink 使用範例

```rust
#[macro_use] extern crate tslink;
use tslink::tslink;

#[tslink(class)]
struct MyVec {
    pub data: Vec<i32>,
}

#[tslink]
impl MyVec {
    #[tslink]
    pub fn push(&mut self, v: i32) {}
    #[tslink]
    pub fn get(&self, idx: usize) -> i32 { 0 }
    #[tslink]
    pub fn len(&self) -> usize { 0 }
}
```

產生：

```typescript
export declare class MyVec {
    data: number[];
    push(v: number): void;
    get(idx: number): number;
    len(): number;
}
```

---

## 12. 參考資源

- [rusty_v8](https://github.com/denoland/rusty_v8)
- [tslink](https://github.com/icsmw/tslink)
- [node-bindgen](https://github.com/infinyon/node-bindgen)
- [wasm-bindgen](https://github.com/rustwasm/wasm-bindgen)
- [napi-rs](https://github.com/napi-rs/napi-rs)

---

**如需更進階範例或自動化流程設計，歡迎隨時討論！**
