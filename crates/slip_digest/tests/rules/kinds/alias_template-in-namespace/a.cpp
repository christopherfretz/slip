namespace acme {
template <class T> using P = T *;
}
acme::P<int> use(acme::P<int> p) { return p; }
