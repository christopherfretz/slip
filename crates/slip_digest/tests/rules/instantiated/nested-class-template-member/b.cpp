template <class T> struct O { template <class U> struct I { int g() { return 1; } }; };
int u() { return O<int>::I<char>().g(); }
