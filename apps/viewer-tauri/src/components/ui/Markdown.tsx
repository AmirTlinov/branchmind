import React from "react";

// Lightweight markdown renderer. No external deps.
// Supports: headings, bold, italic, inline code, code blocks,
// bullet/numbered lists, links, horizontal rules.

interface MarkdownProps {
  text: string;
  className?: string;
}

function parseInline(raw: string): React.ReactNode[] {
  const parts: React.ReactNode[] = [];
  // Regex: bold (**), italic (*), inline code (`), links [text](url)
  const re = /(\*\*(.+?)\*\*)|(\*(.+?)\*)|(`([^`]+?)`)|(\[([^\]]+)\]\(([^)]+)\))/g;
  let lastIndex = 0;
  let match: RegExpExecArray | null;
  let key = 0;

  while ((match = re.exec(raw)) !== null) {
    if (match.index > lastIndex) {
      parts.push(raw.slice(lastIndex, match.index));
    }
    if (match[1]) {
      parts.push(<strong key={key++} className="font-semibold text-gray-900">{match[2]}</strong>);
    } else if (match[3]) {
      parts.push(<em key={key++} className="italic text-gray-700">{match[4]}</em>);
    } else if (match[5]) {
      parts.push(
        <code key={key++} className="px-1.5 py-0.5 rounded-md bg-gray-100 text-[11px] font-mono text-gray-800">
          {match[6]}
        </code>,
      );
    } else if (match[7]) {
      parts.push(
        <a key={key++} href={match[9]} className="text-blue-600 hover:underline" target="_blank" rel="noopener noreferrer">
          {match[8]}
        </a>,
      );
    }
    lastIndex = match.index + match[0].length;
  }

  if (lastIndex < raw.length) {
    parts.push(raw.slice(lastIndex));
  }

  return parts.length === 0 ? [raw] : parts;
}

function parseLine(line: string): { type: string; level?: number; content: string; lang?: string } {
  if (/^#{1,6}\s/.test(line)) {
    const m = line.match(/^(#{1,6})\s+(.*)/);
    if (m) return { type: "heading", level: m[1].length, content: m[2] };
  }
  if (/^(\*|-|\+)\s/.test(line)) {
    return { type: "bullet", content: line.replace(/^(\*|-|\+)\s+/, "") };
  }
  if (/^\d+\.\s/.test(line)) {
    return { type: "numbered", content: line.replace(/^\d+\.\s+/, "") };
  }
  if (/^---$|^\*\*\*$|^___$/.test(line.trim())) {
    return { type: "hr", content: "" };
  }
  return { type: "text", content: line };
}

export function Markdown({ text, className }: MarkdownProps) {
  if (!text) return null;

  const lines = text.split("\n");
  const elements: React.ReactNode[] = [];
  let key = 0;
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    // Code block (```)
    if (line.trimStart().startsWith("```")) {
      const lang = line.trimStart().slice(3).trim();
      const codeLines: string[] = [];
      i++;
      while (i < lines.length && !lines[i].trimStart().startsWith("```")) {
        codeLines.push(lines[i]);
        i++;
      }
      i++; // skip closing ```
      elements.push(
        <div key={key++} className="rounded-xl overflow-hidden border border-gray-200/60 my-3">
          {lang && (
            <div className="px-3 py-1.5 bg-gray-900 text-[10px] text-gray-500 font-mono">{lang}</div>
          )}
          <pre className={`${lang ? "" : "rounded-t-xl"} bg-gray-950 p-3.5 text-[11px] text-gray-300 font-mono leading-[1.7] overflow-x-auto`}>
            <code>{codeLines.join("\n")}</code>
          </pre>
        </div>,
      );
      continue;
    }

    const parsed = parseLine(line);

    // Heading
    if (parsed.type === "heading") {
      const sizeClass =
        parsed.level === 1 ? "text-[16px] font-bold mt-5 mb-2" :
        parsed.level === 2 ? "text-[14px] font-bold mt-4 mb-2" :
        "text-[13px] font-bold mt-3 mb-1.5";
      const cls = `${sizeClass} text-gray-900`;
      const children = parseInline(parsed.content);
      elements.push(
        React.createElement(`h${parsed.level}`, { key: key++, className: cls }, ...children),
      );
      i++;
      continue;
    }

    // Horizontal rule
    if (parsed.type === "hr") {
      elements.push(
        <hr key={key++} className="border-none h-px bg-gradient-to-r from-transparent via-gray-200/60 to-transparent my-4" />,
      );
      i++;
      continue;
    }

    // Bullet list
    if (parsed.type === "bullet") {
      const items: string[] = [];
      while (i < lines.length && parseLine(lines[i]).type === "bullet") {
        items.push(parseLine(lines[i]).content);
        i++;
      }
      elements.push(
        <ul key={key++} className="space-y-1 my-2 ml-1">
          {items.map((item, j) => (
            <li key={j} className="flex items-start gap-2 text-[12px] text-gray-700 leading-relaxed">
              <div className="w-1.5 h-1.5 rounded-full bg-gray-300 mt-[7px] shrink-0" />
              <span>{parseInline(item)}</span>
            </li>
          ))}
        </ul>,
      );
      continue;
    }

    // Numbered list
    if (parsed.type === "numbered") {
      const items: string[] = [];
      while (i < lines.length && parseLine(lines[i]).type === "numbered") {
        items.push(parseLine(lines[i]).content);
        i++;
      }
      elements.push(
        <ol key={key++} className="space-y-1 my-2 ml-1">
          {items.map((item, j) => (
            <li key={j} className="flex items-start gap-2 text-[12px] text-gray-700 leading-relaxed">
              <span className="text-gray-400 font-mono text-[11px] mt-[1px] shrink-0 w-4 text-right">{j + 1}.</span>
              <span>{parseInline(item)}</span>
            </li>
          ))}
        </ol>,
      );
      continue;
    }

    // Empty line
    if (line.trim() === "") {
      i++;
      continue;
    }

    // Paragraph — collect consecutive text lines
    const para: string[] = [];
    while (i < lines.length && lines[i].trim() !== "" && parseLine(lines[i]).type === "text") {
      para.push(lines[i]);
      i++;
    }
    if (para.length > 0) {
      elements.push(
        <p key={key++} className="text-[12px] text-gray-700 leading-relaxed my-1.5">
          {parseInline(para.join(" "))}
        </p>,
      );
    }
  }

  return <div className={className}>{elements}</div>;
}
