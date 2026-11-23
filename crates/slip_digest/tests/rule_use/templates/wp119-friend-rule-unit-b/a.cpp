namespace n {
template <class T> struct S { friend int f(S) { return 1; } };
int f(S<int>);
}
int u() { n::S<int> s; return f(s); }
