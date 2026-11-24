namespace a { int g(int x) { return x; } }
template <class T> T t(T x) { return x + a::g(1); }
int u() { return t(1) + (int)t(2L); }
