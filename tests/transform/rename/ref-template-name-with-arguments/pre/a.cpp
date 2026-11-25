namespace a { template <class T> T f(T x) { return x; } }
int u() { return a::f<int>(1) + a::f(2) + (int)a::f<long>(3); }
