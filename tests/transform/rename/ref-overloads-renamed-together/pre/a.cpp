namespace a {
int f(int x) { return x; }
int f(long x) { return (int)x; }
}
int u() { return a::f(1) + a::f(2L); }
