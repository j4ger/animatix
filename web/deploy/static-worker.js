// Reviewed static-only forwarding module. No networking, state, or dynamic routing.
export default {
  fetch(request, env) {
    return env.ASSETS.fetch(request);
  },
};
