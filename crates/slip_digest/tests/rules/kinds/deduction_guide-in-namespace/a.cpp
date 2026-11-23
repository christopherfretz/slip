namespace acme {
template <class T> struct W { W(T); };
W(const char *) -> W<int>;
}
auto w = acme::W(1);
