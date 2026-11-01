import { PageHeader } from "../components/PageHeader";
import { SkinLibrary } from "../components/SkinLibrary";
import type { PageDefinition } from "../navigation";

type SkinsPageProps = {
  page: PageDefinition;
};

/**
 * 皮肤库：左边导航里的独立栏目（以前是账户页里的一个标签）。皮肤是本机的一份收藏，
 * 不属于某个账户；「换上」时才选账户。
 */
export function SkinsPage({ page }: SkinsPageProps) {
  return (
    <>
      <PageHeader title={page.label} description={page.summary} />
      <SkinLibrary />
    </>
  );
}
