export const ACCEPTED_TYPES = ["image/png", "image/jpeg", "image/webp", "application/pdf"];

/** A FileReader data URL is `data:<media>;base64,<payload>`. Return just the payload. */
export function splitDataUrl(dataUrl: string): string {
  const comma = dataUrl.indexOf(",");
  return comma >= 0 ? dataUrl.slice(comma + 1) : dataUrl;
}

export interface UploadFileIn { filename: string; media_type: string; data_base64: string }

/** Read a browser File into the {filename, media_type, data_base64} shape the API expects. */
export function readFileAsUpload(file: File): Promise<UploadFileIn> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(new Error(`failed to read ${file.name}`));
    reader.onload = () => {
      const result = typeof reader.result === "string" ? reader.result : "";
      resolve({ filename: file.name, media_type: file.type || "application/octet-stream", data_base64: splitDataUrl(result) });
    };
    reader.readAsDataURL(file);
  });
}
