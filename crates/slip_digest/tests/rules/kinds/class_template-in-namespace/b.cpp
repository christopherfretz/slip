namespace legacy {
template <class T> struct W { T t; };
template <> struct W<char> { int c; };
template <class T> struct W<T *> { int p; };
W<int> wi;
}
int use(legacy::W<long> *p, legacy::W<int *> q) { return q.p; }
