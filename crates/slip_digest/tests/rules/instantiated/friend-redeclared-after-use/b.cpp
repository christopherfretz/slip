template <class T> struct S { friend int g(S) { return 1; } };
int u() { return g(S<int>()); }
int g(S<int>);
