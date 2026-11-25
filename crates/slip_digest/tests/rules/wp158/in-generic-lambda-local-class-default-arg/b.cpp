struct X { static int f(int) { return 1; } static int g(long) { return 2; } int mf(int) { return 1; } int mg(long) { return 2; } };
struct Y { static int f(long) { return 0; } int mf(long) { return 0; } };
int w() { auto l = [](auto u) { struct L { int g(int k = decltype(u)::f(1L)) { return k; } }; return L().g(); }; return l(X()); }
