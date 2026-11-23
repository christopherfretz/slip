namespace acme { extern "C" int cf(int); }
int use() { return acme::cf(1); }
