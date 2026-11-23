namespace acme { struct S { int x; }; }
template <class T> struct W { T t; };
template <> struct W<acme::S *> { int p; };
W<acme::S> w;
int use(W<acme::S *> q) { return q.p + w.t.x; }
