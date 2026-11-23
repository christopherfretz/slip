template <class T> struct V { T t; };
template <> struct V<char> { int c; };
template <class T> struct V<T *> { int p; };
V<int> wi;
int use(V<long> *p, V<int *> q) { return q.p; }
