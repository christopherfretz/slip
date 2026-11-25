struct X { static int f(int) { return 1; } static int f(long) { return 2; } int mf(int) { return 1; } int mf(long) { return 2; } };
struct Y { static int f(long) { return 0; } int mf(long) { return 0; } };
template <class T> int c(T) { auto o = [] { auto l = [](int k = T::f(1L)) { return k; }; return l(); }; return o(); }
int w() { return c(X()); }
