namespace legacy { struct Mine {}; }
using namespace legacy;
int use() { Mine m; return sizeof m; }
