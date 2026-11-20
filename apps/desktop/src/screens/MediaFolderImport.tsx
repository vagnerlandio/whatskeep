import { mediaApi, SourceApp } from "../api";
import { useImportT } from "../i18n/import";
import { useJob } from "../jobs";

/** Imports the media from a WhatsApp folder copied from the phone into the vault (as an app-wide job). */
export default function MediaFolderImport({ source }: { source: SourceApp }) {
  const t = useImportT();
  const { job, start } = useJob();
  return (
    <button
      className="small-btn"
      disabled={job?.status === "running"}
      title={t.mediaFolderTitle}
      onClick={() =>
        start(
          "media",
          source,
          async () => {
            const media = await mediaApi.importFolder(source);
            return media && { media };
          },
          { steps: ["media"], picksFolder: true },
        )
      }
    >
      {t.mediaFolder}
    </button>
  );
}
