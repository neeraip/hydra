import { useEffect, useState } from "react";

const RELEASES_URL =
  "https://api.github.com/repos/neeraip/hydra/releases/latest";

export type ReleaseInfo =
  | { status: "loading" }
  | { status: "unavailable" }
  | {
      status: "loaded";
      version: string;
      date: string;
      items: string[];
      releaseUrl: string;
    };

function parseItems(body: string): string[] {
  return body
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => /^[-*] /.test(line))
    .map((line) =>
      line
        .replace(/^[-*] /, "")
        // Strip GitHub's auto-generated " (#123) by @user" suffix.
        .replace(/\s*\(#\d+\)\s*by\s*@\S+\s*$/, "")
        .trim(),
    )
    .filter(Boolean);
}

function formatDate(iso: string): string {
  return new Date(iso).toLocaleDateString("en-GB", {
    month: "long",
    year: "numeric",
  });
}

export function useLatestRelease(): ReleaseInfo {
  const [info, setInfo] = useState<ReleaseInfo>({ status: "loading" });

  useEffect(() => {
    let cancelled = false;

    async function load() {
      try {
        const res = await fetch(RELEASES_URL, {
          headers: { Accept: "application/vnd.github+json" },
        });
        if (!res.ok) {
          if (!cancelled) setInfo({ status: "unavailable" });
          return;
        }
        const data = await res.json();
        if (cancelled) return;

        const tag: string = data.tag_name ?? "";
        const version = tag.replace(/^v/, "");
        if (!version) {
          setInfo({ status: "unavailable" });
          return;
        }

        setInfo({
          status: "loaded",
          version,
          date: data.published_at ? formatDate(data.published_at) : "",
          items: data.body ? parseItems(data.body) : [],
          releaseUrl: data.html_url ?? "",
        });
      } catch {
        if (!cancelled) setInfo({ status: "unavailable" });
      }
    }

    load();
    return () => {
      cancelled = true;
    };
  }, []);

  return info;
}
