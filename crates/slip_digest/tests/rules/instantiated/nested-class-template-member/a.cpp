template <class T> struct O { template <class U> struct I { int f() { return 1; } }; };
int u() { return O<int>::I<char>().f(); }
