// Models: every speech model graded for English, German and Arabic, with speed and size. Pick, download, delete.
import { useEffect, useState } from "react";
import { api } from "../../lib/api";
import { PageHeader } from "../../ui";
import { ModelPicker } from "../parts/ModelPicker";

export function Models() {
  const [languages, setLanguages] = useState<string[]>(["en"]);
  useEffect(() => { api.settings().then((s) => setLanguages(s.languages)).catch(() => {}); }, []);
  return (
    <>
      <PageHeader title="Models" description="Grades are for accuracy in each language. Models run on this Mac, so your voice never leaves it." />
      <ModelPicker languages={languages} />
      <p className="mt-3 px-1 text-xs text-muted">Speed bars: 4 is fastest. Delete a model you no longer use to free the space; it can be downloaded again any time.</p>
    </>
  );
}
