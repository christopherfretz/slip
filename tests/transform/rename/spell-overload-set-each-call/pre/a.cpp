namespace a {
int f(int x) { return x; }
int f(long x) { return (int)x + 1; }
int f(const char *) { return 2; }
int u() { return f(1) + f(2L) + f("x"); }
}
