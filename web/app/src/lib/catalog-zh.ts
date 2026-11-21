export const workCopy: Record<string, {title: string; question: string; category: string; description: string; insight: string; note: string}> = {
  'move-the-basis': {
    title: '移动基向量', question: '所有点都遵循同一条规则时，画面会怎么变？', category: '数学',
    description: '水平剪切让网格、两个基向量和一个采样点一起变形。灰色坐标轴固定不动，方便对照变换前后的空间。',
    insight: '矩阵变换了基向量，其他点随之移动。', note: '行列式为 1，面积保持不变。长度和角度会变化，两条坐标轴使用同一比例。',
  },
  'make-room': {
    title: '给新元素腾位置', question: '怎样把新元素插进已排序的一列？', category: '算法',
    description: '插入排序每次处理一个新元素。向左逐个比较，把较大的元素右移，再将新元素放入空位。两个相等的元素保留原来的顺序。',
    insight: '逐步看清每次比较如何决定下一次移动。', note: '演示使用一组固定的五个元素。计数器记录比较次数与右移次数。',
  },
  'one-rule-many-forms': {
    title: '同一形状，不同节奏', question: '同一个形状重复 48 次，会产生什么运动？', category: '视觉节奏',
    description: '每个方块执行相同的动作，只有延迟不同。按离中心的距离延迟，会形成涟漪。按对角线方向延迟，则会依次扫过。',
    insight: '改变延迟规则，就能改变整组动作的节奏。', note: '选择一种起始规则。演示进行到一半时，同一组方块会切换到另一种规则。',
  },
  'when-waves-meet': {
    title: '当两道波相遇', question: '两道波相遇时，什么时候增强，什么时候抵消？', category: '数学',
    description: '先看一个位置，再观察整条曲线。两道等幅的波在每个位置把位移相加。同相时增强，反相时抵消。',
    insight: '合成波来自每个位置上的位移之和。', note: '两道波的频率和振幅相同。初始相位决定起点，随后相位走完一个周期。坐标尺度保持固定。',
  },
  'words-keep-time': {
    title: '一眼看懂维护通知', question: '维护期间，我还能编辑吗？', category: '排版',
    description: '把一段维护公告拆成三条信息：什么会变、何时生效、哪些功能仍可用。',
    insight: '给读者关心的每个问题单独留一行。', note: '演示中的服务通知为虚构示例。',
  },
};

interface LessonCopy { title: string; goal: string; explanation: string; exercise: string }

export const lessonCopy: Record<string, LessonCopy> = {
  'make-something-move': {
    title: '让形状动起来', goal: '设置起点，然后选个时刻让对象改变。',
    explanation: '# 标记指定指令开始的时间，方括号指定它的持续时间，缓入缓出改变沿途的速度。',
    exercise: '改一下终点，再看一遍。观察除了终点之外，哪些地方保持不变。',
  },
  'compose-a-scene': {
    title: '组合场景', goal: '让对象按相互关系排列。',
    explanation: 'Row 排列子对象，gap 控制间距。Group 可以让排好的形状作为整体旋转。',
    exercise: '调整间距，试着让整体构图保持平衡。',
  },
  'repeat-with-intent': {
    title: '用一条规则重复', goal: '写一次规则，用数组把它应用到整个场景。',
    explanation: '循环创建十二个对象。大家都用同一个表达式，索引让每个对象稍晚一点开始。Grid 负责排列。',
    exercise: '调整相邻对象的延迟，看看哪种节奏更容易读懂。',
  },
  'values-drive-motion': {
    title: '用数值驱动运动', goal: '用 always 把连续变化的数值连到画面上。',
    explanation: 'always 根据场景时间 t（秒）求值。正弦和余弦计算圆上的位置，拖到某个时刻与播放到那里会得到同样的位置。',
    exercise: '改变速度，观察圆和点之间的关系。',
  },
  'structure-time': {
    title: '安排时间', goal: '用时间标记隔开动作，为观众留出阅读时间。',
    explanation: '三个绝对时间标记依次安排入场，之后对象一起移动。两个动作之间可以留白。',
    exercise: '延长最后一次入场后的停顿。画面更容易读懂了吗？',
  },
  'finish-and-share': {
    title: '完成与分享', goal: '做一张能独立阅读的结束画面，并带走源码。',
    explanation: '分辨率和时长定义输出。结尾保留主体，最后一帧可以直接作为静图使用。',
    exercise: '下载 AMX，用本地工具打开。音频和导出功能取决于安装的构建版本。',
  },
};

export const techniqueCopy: Record<string, LessonCopy & {rule: string; alt: string}> = {
  reveal: {
    title: '逐步呈现关系', goal: '每次只向观众展示一层关系。',
    explanation: '先出现对象，再出现名字，最后才画出连线。分开的 # 标记方便调整阅读顺序。',
    exercise: '把连线提前一点。观众还有时间看清端点吗？',
    rule: '在结束帧保留完整图示。', alt: '青色大点与金色小点由细线相连，标签位于各自下方。',
  },
  focus: {
    title: '引导注意力', goal: '强调其中一个元素，同时保留上下文。',
    explanation: '降低周围元素的透明度，用颜色和下划线标出当前对象，三个元素的位置不动。',
    exercise: '换一个被强调的元素，使下划线与明暗变化同步。',
    rule: '确保画面上有一个明确的焦点。', alt: '一排三个方块，右侧金色方块带有下划线，另外两个变暗。',
  },
  stagger: {
    title: '安排错峰节奏', goal: '让重复的入场动作形成阅读顺序。',
    explanation: 'stagger 将五个 600 毫秒的淡入依次推迟 300 毫秒。间隔决定何时开始，淡入时长保持相同。',
    exercise: '分别改为 150 毫秒和 600 毫秒，比较动作的重叠程度。',
    rule: '用时间上的错开交代结构。', alt: '五个青色圆形以相同间距排成一行。',
  },
  labels: {
    title: '让标签跟随对象', goal: '让运动中的主体与注释保持位置关系。',
    explanation: '点、引线和标签共用一个随时间变化的横坐标。纵向间距固定，移动结束后画面保持可读。',
    exercise: '修改共享表达式里的终点，检查三者是否仍然对齐。',
    rule: '相关位置用同一个表达式派生。', alt: '水平轨道右端有一个点，竖直引线通向上方的标签。',
  },
};
