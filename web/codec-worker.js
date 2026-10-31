// Decodes and encodes away from the page, because a large ROM takes a while.
importScripts("gbadn.js");

const ready = createGbadn();

function text(gbadn, pointer, size) {
  return new TextDecoder().decode(gbadn.HEAPU8.subarray(pointer, pointer + size));
}

onmessage = async ({ data: { id, action, bytes } }) => {
  const gbadn = await ready;
  const input = gbadn._malloc(bytes.length);
  gbadn.HEAPU8.set(bytes, input);
  const run = action === "encode" ? gbadn._gbadn_web_encode : gbadn._gbadn_web_decode;
  const status = run(input, bytes.length);
  gbadn._free(input);

  const message = text(gbadn, gbadn._gbadn_web_message(), gbadn._gbadn_web_message_size());
  if (status !== 0) {
    postMessage({ id, error: message });
    return;
  }
  const pointer = gbadn._gbadn_web_bytes();
  const result = gbadn.HEAPU8.slice(pointer, pointer + gbadn._gbadn_web_bytes_size());
  postMessage({ id, message, bytes: result }, [result.buffer]);
};
