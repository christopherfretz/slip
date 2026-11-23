template <class T> struct A { friend int f(int); };
int f(int) { return 1; }
A<int> a;
int u() { return f(1); }
