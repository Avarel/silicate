use wasm_bindgen_futures::JsFuture;
use web_sys::js_sys::ArrayBuffer;
use web_sys::js_sys::wasm_bindgen::JsCast as _;
use web_sys::js_sys::{self, wasm_bindgen::JsValue};
use web_sys::{Request, RequestInit, RequestMode, Response};

async fn fetch_blob(resource_name: &str) -> Result<Vec<u8>, JsValue> {
    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);

    let request = Request::new_with_str_and_init(resource_name, &opts)?;

    request
        .headers()
        .set("Accept", "application/octet-stream")?;

    let window = web_sys::window().expect("no global `window` exists");
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;

    let resp: Response = resp_value.dyn_into().expect("Response object expected");

    let array_buf = JsFuture::from(resp.array_buffer()?).await?;
    assert!(array_buf.is_instance_of::<ArrayBuffer>());

    let typebuf: js_sys::Uint8Array = js_sys::Uint8Array::new(&array_buf);
    let body = typebuf.to_vec();

    Ok(body)
}

pub async fn save_blob_as_png(blob: &[u8]) -> Result<(), JsValue> {
    let byte_array = js_sys::Uint8Array::from(blob);
    log::debug!(
        "Created Uint8Array from blob, length: {}",
        byte_array.length()
    );

    let array = {
        let array = js_sys::Array::new();
        array.push(&byte_array);
        array
    };

    let options = {
        let options = web_sys::BlobPropertyBag::new();
        options.set_type("image/png");
        options
    };

    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&array, &options)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)?;
    let window = web_sys::window().expect("no global `window` exists");

    let document = window.document().expect("no document on window");
    let link = {
        let link = document
            .create_element("a")?
            .dyn_into::<web_sys::HtmlAnchorElement>()?;
        link.set_href(&url);
        link.set_download("image.png");
        link.set_inner_text("Click here to download the file");
        link
    };
    let body = document.body().expect("document should have a body");
    let node = body.append_child(&link)?;
    link.click();

    web_sys::Url::revoke_object_url(&url)?;

    body.remove_child(&node)?;

    Ok(())
}

pub async fn fetch_demo_file() -> Result<Vec<u8>, JsValue> {
    fetch_blob("demo/reference.procreate").await
}
