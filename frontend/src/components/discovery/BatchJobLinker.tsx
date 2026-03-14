import { useMemo } from 'react';
import { Clock, Link, Unlink, Calendar } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { useDiscoveryStore } from '@/stores/discovery';

interface BatchJobLinkerProps {
  serviceIndex: number;
}

export function BatchJobLinker({ serviceIndex }: BatchJobLinkerProps) {
  const correlationResult = useDiscoveryStore((s) => s.correlationResult);
  const batchJobLinks = useDiscoveryStore((s) => s.batchJobLinks);
  const linkBatchJob = useDiscoveryStore((s) => s.linkBatchJob);
  const unlinkBatchJob = useDiscoveryStore((s) => s.unlinkBatchJob);

  const scheduledJobs = useMemo(
    () => correlationResult?.scheduled_jobs || [],
    [correlationResult]
  );

  // Get jobs linked to this service
  const linkedJobs = useMemo(() => {
    const jobs: Array<{ index: number; name: string; schedule: string }> = [];
    batchJobLinks.forEach((svcIdx, jobIdx) => {
      if (svcIdx === serviceIndex && scheduledJobs[jobIdx]) {
        jobs.push({
          index: jobIdx,
          name: scheduledJobs[jobIdx].name,
          schedule: scheduledJobs[jobIdx].schedule,
        });
      }
    });
    return jobs;
  }, [batchJobLinks, serviceIndex, scheduledJobs]);

  // Get unlinked jobs
  const unlinkedJobs = useMemo(() => {
    return scheduledJobs
      .map((job, i) => ({ ...job, index: i }))
      .filter((job) => !batchJobLinks.has(job.index));
  }, [scheduledJobs, batchJobLinks]);

  if (scheduledJobs.length === 0) {
    return null;
  }

  const handleLink = (jobIndex: string) => {
    if (jobIndex) {
      linkBatchJob(Number(jobIndex), serviceIndex);
    }
  };

  const handleUnlink = (jobIndex: number) => {
    unlinkBatchJob(jobIndex);
  };

  return (
    <div>
      <div className="text-[10px] font-medium text-muted-foreground uppercase tracking-wider mb-2 flex items-center gap-1">
        <Clock className="h-3 w-3 text-amber-500" />
        LINKED BATCH JOBS ({linkedJobs.length})
      </div>

      <div className="space-y-1.5 pl-2 border-l-2 border-border">
        {/* Linked jobs */}
        {linkedJobs.map((job) => (
          <div key={job.index} className="flex items-center gap-1.5 text-[11px] group">
            <Calendar className="h-3 w-3 text-amber-500 flex-shrink-0" />
            <span className="font-medium truncate flex-1" title={job.name}>
              {job.name}
            </span>
            <Badge variant="secondary" className="text-[9px] h-4 px-1 font-mono">
              {job.schedule}
            </Badge>
            <Button
              size="icon"
              variant="ghost"
              className="h-5 w-5 opacity-0 group-hover:opacity-100 text-muted-foreground hover:text-destructive"
              onClick={() => handleUnlink(job.index)}
              title="Unlink job"
            >
              <Unlink className="h-3 w-3" />
            </Button>
          </div>
        ))}

        {/* Link selector */}
        {unlinkedJobs.length > 0 && (
          <div className="flex items-center gap-1.5 mt-2">
            <Select value="" onValueChange={handleLink}>
              <SelectTrigger className="h-6 text-[10px] flex-1">
                <div className="flex items-center gap-1">
                  <Link className="h-3 w-3 text-muted-foreground" />
                  <SelectValue placeholder="Link a batch job..." />
                </div>
              </SelectTrigger>
              <SelectContent>
                {unlinkedJobs.map((job) => (
                  <SelectItem key={job.index} value={String(job.index)}>
                    <span className="flex items-center gap-1.5">
                      <Clock className="h-3 w-3 text-amber-500" />
                      <span className="truncate">{job.name}</span>
                      <span className="text-[9px] text-muted-foreground font-mono">
                        {job.schedule}
                      </span>
                    </span>
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        )}

        {/* Empty state */}
        {linkedJobs.length === 0 && unlinkedJobs.length === 0 && (
          <div className="text-[11px] text-muted-foreground italic">
            No batch jobs available
          </div>
        )}

        {linkedJobs.length === 0 && unlinkedJobs.length > 0 && (
          <div className="text-[11px] text-muted-foreground italic mb-1">
            No batch jobs linked to this service
          </div>
        )}
      </div>
    </div>
  );
}
