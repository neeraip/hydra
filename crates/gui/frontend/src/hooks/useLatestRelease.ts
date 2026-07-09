import { useEffect, useState } from "react";

const RELEASES_URL = "https://api.github.com/repos/neeraip/hydra/releases";
const RELEASES_PAGE_SIZE = 5;
const RELEASES_MAX_PAGES = 4;

type GitHubRelease = {
  tag_name: string;
  draft: boolean;
  prerelease: boolean;
  published_at?: string;
  body?: string;
  html_url?: string;
};

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
        for (let page = 1; page <= RELEASES_MAX_PAGES; page += 1) {
          const params = new URLSearchParams({
            per_page: String(RELEASES_PAGE_SIZE),
            page: String(page),
          });
          const res = await fetch(`${RELEASES_URL}?${params.toString()}`, {
            headers: { Accept: "application/vnd.github+json" },
          });
          if (!res.ok) {
            if (!cancelled) setInfo({ status: "unavailable" });
            return;
          }

          const releases: GitHubRelease[] = await res.json();
          if (cancelled) return;

          // Find the latest published (non-draft, non-prerelease) gui-v* release.
          const data = releases.find(
            (r) => r.tag_name.startsWith("gui-v") && !r.draft && !r.prerelease,
          );

          if (data) {
            const version = data.tag_name.replace(/^gui-v/, "");
            setInfo({
              status: "loaded",
              version,
              date: data.published_at ? formatDate(data.published_at) : "",
              items: data.body ? parseItems(data.body) : [],
              releaseUrl: data.html_url ?? "",
            });
            return;
          }

          if (releases.length < RELEASES_PAGE_SIZE) {
            break;
          }
        }

        setInfo({ status: "unavailable" });
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
