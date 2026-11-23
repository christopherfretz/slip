#define LOG(x) log_impl(#x)
int log_impl(const char *) { return 0; }
int LOG(int) { return 1; }

int caller() { return LOG(3); }
