#define LOG(x) log_impl(#x)
int log_impl(const char *) { return 0; }
int f(int) { return 1; }

int caller() { return f(3); }
