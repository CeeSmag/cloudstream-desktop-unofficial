import { CloudflareNeeded } from '../types';

/** What to do with an action that failed. */
export type FailureAction = 'ask-to-verify' | 'show-error';

/**
 * A site that is waiting for the viewer's go-ahead for its Cloudflare check explains an action's failure
 * better than the plugin's own message ("returned nothing"), so the viewer is asked to verify instead.
 */
export function failureAction(needed: CloudflareNeeded | null): FailureAction {
  return needed ? 'ask-to-verify' : 'show-error';
}
