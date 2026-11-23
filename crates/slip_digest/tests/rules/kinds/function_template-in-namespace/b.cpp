namespace legacy {
struct S {};
template <class T> int f(T) { return 1; }
template <> int f(S) { return 2; }
}
int use() { return legacy::f(1) + legacy::f(legacy::S()); }
