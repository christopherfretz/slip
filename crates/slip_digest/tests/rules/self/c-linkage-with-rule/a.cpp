namespace acme { extern "C" int cf(int); int g() { return cf(2); } }
int use() { return acme::cf(1) + acme::g(); }
