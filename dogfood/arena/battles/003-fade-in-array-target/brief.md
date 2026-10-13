# Dogfood Project Brief

## Content Goal

Present five numbered cards in a single row. After a short pause, the whole
group appears together with a smooth fade.

## Audience

A language designer evaluating how entrance actions express generated actor
sets.

## Scenes

One scene, no scene transition.

## Constraints

- Target resolution: 1280x720.
- Visual style: `editorial-dark`.
- Five cards, each 160x90.
- Cards are generated with `for`, not hand-named.
- Both variants must show all five cards fully visible after the same
  interval.
- Grammar features under test: generated array actors, indexed action targets,
  and container group-target expansion for entrance actions.

## Success Criteria

A reviewer can tell whether the group is the unit being animated, and which
expression scales better when the generated set changes.
