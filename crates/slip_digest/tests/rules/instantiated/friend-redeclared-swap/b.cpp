template <class T> struct S { friend int g(S) { return 1; } friend int f(S) { return 2; } };
int f(S<int>);
int g(S<int>);
int u() { return f(S<int>()); }
