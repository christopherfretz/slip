struct X { int mf(long) { return 2; } };
struct Y { int mg(long) { return 3; } };
int w() { auto l = [](auto u) { if constexpr (requires { u.mg(1L); }) return u.mg(1L); else return 0; }; return l(X()) + l(Y()); }
