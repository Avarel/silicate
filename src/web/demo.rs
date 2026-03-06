use web_sys::js_sys::ArrayBuffer;
use web_sys::js_sys::wasm_bindgen::JsCast as _;
use wasm_bindgen_futures::JsFuture;
use web_sys::js_sys::{self, wasm_bindgen::JsValue};
use web_sys::{RequestInit, RequestMode, Request, Response};

async fn fetch_as_vec_u8(resource_name: &str) -> Result<Vec<u8>, JsValue> {
    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);

    let request = Request::new_with_str_and_init(resource_name, &opts)?;

    request.headers().set("Accept", "application/octet-stream")?;

    let window = web_sys::window().unwrap();
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;

    let resp: Response = resp_value.dyn_into().expect("Response object expected");

    let array_buf = JsFuture::from(resp.array_buffer()?).await?;
    assert!(array_buf.is_instance_of::<ArrayBuffer>());

    let typebuf: js_sys::Uint8Array = js_sys::Uint8Array::new(&array_buf);
    let body = typebuf.to_vec();

    Ok(body)
}

pub async fn fetch_demo_file() -> Result<Vec<u8>, JsValue> {
    fetch_as_vec_u8("demo/reference.procreate").await
}
