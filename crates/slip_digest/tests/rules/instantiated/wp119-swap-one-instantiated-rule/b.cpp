template <class T> struct S { friend int g(S) { return 1; } friend int f(S) { return 2; } };
int g(S<int>);
int f(S<int>);
int u() { return g(S<int>()); }
