namespace legacy {
struct S { int v = 1; };
template <class T> int f(T t = T()) { return t.v; }
}
int use() { return legacy::f<legacy::S>(); }
