/**
 * Convenience re-exports for enrollment token management hooks.
 *
 * All query/mutation logic lives in @/api/enrollment — this file
 * provides a shorter import path consistent with the hooks/ convention.
 */
export {
  useEnrollmentTokens,
  useCreateEnrollmentToken,
  useRevokeEnrollmentToken,
  useEnrollmentEvents,
} from '@/api/enrollment';

export type {
  EnrollmentToken,
  CreateEnrollmentTokenPayload,
  CreateEnrollmentTokenResponse,
  EnrollmentEvent,
} from '@/api/enrollment';
