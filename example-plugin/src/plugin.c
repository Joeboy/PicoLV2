int return_23(void) {
    return 23;
}

// Passing parameters from host to plugin, and a return value back to host.
int add(int a, int b) {
    return a + b;
}

// A function private to the plugin, only reachable via a call from another
// plugin function - exercises calls/parameter passing/return values that
// stay entirely within the plugin. `noinline` forces a real call (with its
// own relocated branch) instead of being folded into its caller.
__attribute__((noinline)) static int inc(int x) {
    return x + 1;
}

// Calls another function within the plugin, passing a parameter to it and
// returning its result back to the host.
int add_one(int x) {
    return inc(x);
}

// Passing a pointer/buffer from host to plugin: the plugin reads memory it
// didn't allocate, owned by the host.
int sum_buffer(const int *buf, int len) {
    int total = 0;
    for (int i = 0; i < len; i++) {
        total += buf[i];
    }
    return total;
}

// Not defined by the plugin - resolved at load time to a function the host
// provides. Exercises the plugin calling back into the host.
extern int host_double(int x);

int double_via_host(int x) {
    return host_double(x);
}
