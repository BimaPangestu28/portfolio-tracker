import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

/**
 * Renders assistant chat content as GitHub-flavored markdown.
 *
 * react-markdown builds React elements (no dangerouslySetInnerHTML), so output is
 * XSS-safe by default. Raw HTML embedded in the text is escaped, not executed.
 * Links are forced to open in a new tab with rel="noreferrer".
 *
 * @param content - The markdown source to render (string)
 * @returns A `<div className="md">` wrapping the rendered markdown
 *
 * @example
 * <MarkdownMessage content={"**XIRR:** 12.4%\n\n- Saham: 60%\n- Obligasi: 40%"} />
 */
export function MarkdownMessage({ content }: { content: string }) {
  return (
    <div className="md">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          a: ({ node: _node, ...props }) => (
            <a {...props} target="_blank" rel="noreferrer" />
          ),
        }}
      >
        {content}
      </ReactMarkdown>
    </div>
  );
}
