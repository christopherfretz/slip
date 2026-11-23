namespace n { template <class T> void f(T); template <class T> void f(T, int); }
template <class T> void n::f(T) {}
template <> void n::f<int>(int);
template <> void n::f(long) {}
template <> void n::f<int>(int) {}
extern template void n::f<char>(char);
template void n::f<char>(char);
void h() { n::f(1); n::f<short>(2); n::f(1.0, 1); void (*p)(double) = &n::f<double>; (void)p; }
