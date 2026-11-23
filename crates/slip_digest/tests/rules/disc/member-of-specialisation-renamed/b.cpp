namespace legacy {
template <class T> struct W { int m() { return 1; } };
template <> struct W<int> { int n() { return 2; } };
}
int use() { return legacy::W<int>().n() + legacy::W<char>().m(); }
