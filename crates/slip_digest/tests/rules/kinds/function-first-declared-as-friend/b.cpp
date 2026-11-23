template <class T> struct A { friend int g(int); };
int g(int) { return 1; }
A<int> a;
int u() { return g(1); }
