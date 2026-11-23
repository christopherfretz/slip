namespace n {
template <class T> struct S { friend int g(S) { return 1; } };
int g(S<int>);
}
