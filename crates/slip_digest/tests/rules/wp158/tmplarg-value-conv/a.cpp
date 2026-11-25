struct X { static constexpr int f(int) { return 1; } static constexpr int f(long) { return 2; } constexpr int mf(int) { return 1; } constexpr int mf(long) { return 2; } };
template <int N> struct I { static constexpr int v = N; };
int w() { constexpr auto l = [](auto u) { return u.mf(1L); }; return I<static_cast<int (*)(X)>(l)(X())>::v; }
