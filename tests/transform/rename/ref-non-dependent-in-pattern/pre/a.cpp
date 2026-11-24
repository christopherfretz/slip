namespace a { int f(int x) { return x; } }
template <class T> T t(T x) { return x + a::f(1); }
int u() { return t(1) + (int)t(2L); }
