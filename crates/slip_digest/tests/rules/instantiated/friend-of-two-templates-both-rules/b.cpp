template <class T> struct S { friend int g(int) { return 1; } };
template <class T> struct V { friend int g(int); };
int g(int);
S<int> s;
V<int> v;
int u() { return g(1); }
