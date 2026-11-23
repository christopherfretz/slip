namespace legacy { extern "C" int cf(int); int g() { return cf(2); } }
int use() { return legacy::cf(1) + legacy::g(); }
