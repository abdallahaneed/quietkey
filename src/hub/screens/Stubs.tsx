// Screens that are routed but not built yet. Their backend commands already exist (see lib/api.ts).
import { BarChart3 } from "lucide-react";
import type { ReactNode } from "react";
import { EmptyState, PageHeader } from "../../ui";

function Stub({ title, description, icon }: { title: string; description: string; icon: ReactNode }) {
  return (
    <>
      <PageHeader title={title} description={description} />
      <EmptyState icon={icon} title="Coming soon" body="This screen is not built yet." />
    </>
  );
}

export const Insights = () => (
  <Stub title="Insights" description="How much you dictate and how fast." icon={<BarChart3 className="size-8" />} />
);
