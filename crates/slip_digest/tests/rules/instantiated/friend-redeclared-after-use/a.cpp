template <class T> struct S { friend int f(S) { return 1; } };
int u() { return f(S<int>()); }
int f(S<int>);
