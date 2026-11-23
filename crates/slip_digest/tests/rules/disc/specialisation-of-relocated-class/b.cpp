namespace legacy { struct S { int x; }; }
template <class T> struct W { T t; };
template <> struct W<legacy::S *> { int p; };
W<legacy::S> w;
int use(W<legacy::S *> q) { return q.p + w.t.x; }
