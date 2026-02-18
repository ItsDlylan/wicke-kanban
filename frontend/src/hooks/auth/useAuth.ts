export function useAuth() {
  return { isSignedIn: false, isLoaded: true, userId: null } as const;
}
