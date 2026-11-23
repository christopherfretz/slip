template <class T> struct O { template <class U> int p(U) { return 2; } template <class U> int q(U) { return 1; } };
int u() { return O<int>().q(1); }
