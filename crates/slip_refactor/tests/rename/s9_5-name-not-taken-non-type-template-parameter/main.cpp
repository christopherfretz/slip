namespace n { template <int N> int k() { return N; } void f(int); }
void h() { n::f(1); }
