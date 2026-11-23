template <class T> struct S { friend int f(S) { return 1; } };
int f(S<int>);
int u() { return f(S<int>()) + f(S<long>()); }
