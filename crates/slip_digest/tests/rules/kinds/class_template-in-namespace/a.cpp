namespace acme {
template <class T> struct W { T t; };
template <> struct W<char> { int c; };
template <class T> struct W<T *> { int p; };
W<int> wi;
}
int use(acme::W<long> *p, acme::W<int *> q) { return q.p; }
