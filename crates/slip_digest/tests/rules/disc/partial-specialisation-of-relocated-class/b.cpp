namespace legacy { struct S {}; }
template <class T, class U> struct Q { int a; };
template <class T> struct Q<T, legacy::S> { int b; };
template <class T> struct Q<T *, legacy::S> { int c; };
int use(Q<int, legacy::S> q, Q<int *, legacy::S> r) { return q.b + r.c; }
