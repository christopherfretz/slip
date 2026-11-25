namespace a { template <class T> T g(T x) { return x; } }
int u() { return a::g<int>(1) + a::g(2) + (int)a::g<long>(3); }
