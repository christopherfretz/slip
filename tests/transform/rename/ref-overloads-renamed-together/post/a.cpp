namespace a {
int g(int x) { return x; }
int g(long x) { return (int)x; }
}
int u() { return a::g(1) + a::g(2L); }
