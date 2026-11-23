template <class T> struct O { template <class U> int p(U) { return 1; } template <class U> int q(U) { return 2; } };
int u() { return O<int>().p(1); }
