import React from 'react';
import { Composition, registerRoot } from 'remotion';
import { Overview } from './compositions/Overview';
import { CreateEpic } from './compositions/CreateEpic';
import { PlanWithClaude } from './compositions/PlanWithClaude';
import { SpecDecompose } from './compositions/SpecDecompose';
import { RalphExecutes } from './compositions/RalphExecutes';

const FPS = 60;
const WIDTH = 1280;
const HEIGHT = 720;

const RemotionRoot: React.FC = () => {
  return (
    <>
      <Composition
        id="Overview"
        component={Overview}
        durationInFrames={10 * FPS}
        fps={FPS}
        width={WIDTH}
        height={HEIGHT}
      />
      <Composition
        id="CreateEpic"
        component={CreateEpic}
        durationInFrames={10 * FPS}
        fps={FPS}
        width={WIDTH}
        height={HEIGHT}
      />
      <Composition
        id="PlanWithClaude"
        component={PlanWithClaude}
        durationInFrames={10 * FPS}
        fps={FPS}
        width={WIDTH}
        height={HEIGHT}
      />
      <Composition
        id="SpecDecompose"
        component={SpecDecompose}
        durationInFrames={10 * FPS}
        fps={FPS}
        width={WIDTH}
        height={HEIGHT}
      />
      <Composition
        id="RalphExecutes"
        component={RalphExecutes}
        durationInFrames={10 * FPS}
        fps={FPS}
        width={WIDTH}
        height={HEIGHT}
      />
    </>
  );
};

registerRoot(RemotionRoot);
