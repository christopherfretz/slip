namespace acme { struct S {}; }
template <class T, class U> struct Q { int a; };
template <class T> struct Q<T, acme::S> { int b; };
template <class T> struct Q<T *, acme::S> { int c; };
int use(Q<int, acme::S> q, Q<int *, acme::S> r) { return q.b + r.c; }
