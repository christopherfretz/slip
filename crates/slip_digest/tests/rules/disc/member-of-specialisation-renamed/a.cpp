namespace acme {
template <class T> struct W { int m() { return 1; } };
template <> struct W<int> { int m() { return 2; } };
}
int use() { return acme::W<int>().m() + acme::W<char>().m(); }
