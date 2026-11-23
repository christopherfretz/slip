namespace acme { struct Mine {}; }
using namespace acme;
int use() { Mine m; return sizeof m; }
