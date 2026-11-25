namespace a {
inline namespace v1 { int g(long x) { return (int)x; } }
int f(int x) { return x; }
}
int u() { return a::f(1) + a::g(2L); }
