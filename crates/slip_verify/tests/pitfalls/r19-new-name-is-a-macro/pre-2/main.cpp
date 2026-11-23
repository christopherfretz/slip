#define LOG(x) log_impl(x)
long log_impl(long) { return 2; }
int f(int) { return 1; }

long caller() { return f(3); }
long other() { return log_impl(1); }
