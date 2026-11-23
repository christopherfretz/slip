template <class T> struct S { friend int f(int) { return 1; } };
template <class T> struct V { friend int f(int); };
int f(int);
S<int> s;
V<int> v;
int u() { return f(1); }
