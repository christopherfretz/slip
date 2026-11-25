struct X { int mf(long) { return 2; } };
struct Y { int mf(long) { return 3; } };
int w() { auto l = [](auto u) { if constexpr (requires { u.mf(1L); }) return u.mf(1L); else return 0; }; int (*p)(X) = l; return p(X()) + l(Y()); }
