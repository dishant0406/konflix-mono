import { Spinner } from "@/components/states";

export default function Loading() {
  return (
    <div className="flex h-[60vh] items-center justify-center">
      <Spinner />
    </div>
  );
}
