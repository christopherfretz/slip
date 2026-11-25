struct X { static constexpr int f(int) { return 1; } static constexpr int g(long) { return 2; } constexpr int mf(int) { return 1; } constexpr int mg(long) { return 2; } };
int w() { auto l = [](auto u) { return u.mf(1L); }; constexpr int (*p)(X) = l; char a[p(X())]; return sizeof a; }
